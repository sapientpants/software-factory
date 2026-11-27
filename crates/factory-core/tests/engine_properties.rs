//! Property tests of the engine over event sequences, for the invariants of
//! SPEC 3.1 that the row "Engine" of 19.4 lists: one active run per issue,
//! round caps respected, no merge effect without a valid approval, and a
//! duplicated or re-delivered result event leaves the state unchanged.
//!
//! A sequence is generated as abstract actions, and a driver makes each one
//! a concrete event against what the engine has asked for so far: it
//! completes a write that the engine requested, ends an activity that the
//! engine started, and so on. The driver keeps, beside the engine, its own
//! record of what was pushed and approved, which the merge property is
//! checked against.

use factory_core::engine::{
    ActivityId, ActivityOutcome, Actor, ActorKind, CancelCause, CiConclusion, Effect, EffectId,
    EngineConfig, Event, IssueKey, MergeStrategy, RepoId, ReviewId, ReviewVerdict, RoundCaps,
    RunId, Sha, Stage, State, Timestamp, Write, WriteOutcome, step,
};
use proptest::prelude::*;
use proptest::sample::Index;
use std::collections::{BTreeMap, BTreeSet};

/// What a generated step of the sequence does.
#[derive(Debug, Clone)]
enum Action {
    /// Claim one of three issues, with a new run id.
    Claim(u8),
    /// Complete one of the writes that have no result yet.
    Complete(Index, bool),
    /// End one of the live activities.
    End(Index, Outcome),
    /// Abandon one of the live activities.
    Abandon(Index),
    /// Conclude the checks of a run, on its head or on another commit.
    Checks(Index, bool, bool),
    /// Submit a review on a run.
    Review(Index, ReviewChoice),
    /// Merge a run's pull request.
    Merged(Index),
    /// Cancel a run.
    Cancel(Index),
    /// Deliver a result event that was delivered before, again.
    Redeliver(Index),
}

#[derive(Debug, Clone, Copy)]
enum Outcome {
    /// The success of the activity's stage: a candidate for `capture`.
    Passed,
    GateFailed,
    NeedsHuman,
    Failed,
    /// The success of another stage, which this one cannot have.
    Mismatched,
}

#[derive(Debug, Clone)]
struct ReviewChoice {
    id: u8,
    verdict: ReviewVerdict,
    kind: ActorKind,
    write_permission: bool,
    on_head: bool,
}

fn action() -> impl Strategy<Value = Action> {
    let outcome = prop_oneof![
        24 => Just(Outcome::Passed),
        4 => Just(Outcome::GateFailed),
        1 => Just(Outcome::NeedsHuman),
        1 => Just(Outcome::Failed),
        1 => Just(Outcome::Mismatched),
    ];
    let verdict = prop_oneof![
        Just(ReviewVerdict::Approved),
        Just(ReviewVerdict::ChangesRequested),
        Just(ReviewVerdict::Commented),
    ];
    let kind = prop_oneof![4 => Just(ActorKind::User), 1 => Just(ActorKind::Bot)];
    let review = (
        any::<u8>(),
        verdict,
        kind,
        prop::bool::weighted(0.8),
        prop::bool::weighted(0.8),
    )
        .prop_map(
            |(id, verdict, kind, write_permission, on_head)| ReviewChoice {
                id: id % 16,
                verdict,
                kind,
                write_permission,
                on_head,
            },
        );
    prop_oneof![
        4 => (0u8..3).prop_map(Action::Claim),
        48 => (any::<Index>(), prop::bool::weighted(0.99)).prop_map(|(i, ok)| Action::Complete(i, ok)),
        48 => (any::<Index>(), outcome).prop_map(|(i, o)| Action::End(i, o)),
        2 => any::<Index>().prop_map(Action::Abandon),
        12 => (any::<Index>(), prop::bool::weighted(0.7), prop::bool::weighted(0.9))
            .prop_map(|(i, green, on_head)| Action::Checks(i, green, on_head)),
        16 => (any::<Index>(), review).prop_map(|(i, r)| Action::Review(i, r)),
        1 => any::<Index>().prop_map(Action::Merged),
        1 => any::<Index>().prop_map(Action::Cancel),
        8 => any::<Index>().prop_map(Action::Redeliver),
    ]
}

/// The driver: the engine's state, what it asked for, and the record that
/// the merge property is checked against.
struct Driver {
    state: State,
    next_run: u128,
    next_candidate: u32,
    /// Writes that have no result yet.
    writes: Vec<(EffectId, Write)>,
    /// Activities that were started and have not ended.
    activities: Vec<ActivityId>,
    /// The stage of every activity that was started.
    stages: BTreeMap<ActivityId, Stage>,
    /// Result events that were delivered.
    delivered: Vec<Event>,
    /// Per run, the SHA of each push whose result was a success, the last
    /// one last.
    pushes: BTreeMap<RunId, Vec<Sha>>,
    /// Per run, the head SHAs on which a review that counts under PR-2
    /// approved, since the last successful push.
    approved: BTreeMap<RunId, BTreeSet<Sha>>,
    /// Per run, the live activity that the effects say it has.
    live: BTreeMap<RunId, ActivityId>,
}

impl Driver {
    fn new(merge_strategy: MergeStrategy) -> Self {
        Self {
            state: State::new(EngineConfig {
                caps: RoundCaps::default(),
                merge_strategy,
            }),
            next_run: 1,
            next_candidate: 0,
            writes: Vec::new(),
            activities: Vec::new(),
            stages: BTreeMap::new(),
            delivered: Vec::new(),
            pushes: BTreeMap::new(),
            approved: BTreeMap::new(),
            live: BTreeMap::new(),
        }
    }

    /// One of the active runs.
    fn pick_run(&self, index: &Index) -> Option<RunId> {
        let runs: Vec<RunId> = self
            .state
            .runs
            .values()
            .filter(|run| !run.is_terminal())
            .map(|run| run.id)
            .collect();
        (!runs.is_empty()).then(|| runs[index.index(runs.len())])
    }

    fn candidate(&mut self) -> ActivityOutcome {
        self.next_candidate += 1;
        ActivityOutcome::Captured(Sha(format!("c{}", self.next_candidate)))
    }

    /// The event of an action, or `None` where it has nothing to act on.
    fn event(&mut self, action: &Action) -> Option<Event> {
        Some(match action {
            Action::Claim(issue) => {
                let run = RunId(self.next_run);
                self.next_run += 1;
                Event::Claim {
                    run,
                    issue: IssueKey {
                        repo: RepoId(1),
                        number: u64::from(*issue),
                    },
                    base_sha: Sha("base".to_owned()),
                    at: Timestamp(0),
                }
            }
            Action::Complete(index, ok) => {
                if self.writes.is_empty() {
                    return None;
                }
                let (effect, write) = self.writes.remove(index.index(self.writes.len()));
                if *ok && let Write::Push { candidate } = write {
                    self.pushes.entry(effect.run).or_default().push(candidate);
                    self.approved.remove(&effect.run);
                }
                Event::WriteCompleted {
                    effect,
                    outcome: if *ok {
                        WriteOutcome::Succeeded
                    } else {
                        WriteOutcome::Failed
                    },
                }
            }
            Action::End(index, outcome) => {
                if self.activities.is_empty() {
                    return None;
                }
                let activity = self.activities.remove(index.index(self.activities.len()));
                let capture = self.stages[&activity] == Stage::Capture;
                let outcome = match outcome {
                    Outcome::Passed if capture => self.candidate(),
                    Outcome::Mismatched if !capture => self.candidate(),
                    Outcome::Passed | Outcome::Mismatched => ActivityOutcome::Succeeded,
                    Outcome::GateFailed => ActivityOutcome::GateFailed,
                    Outcome::NeedsHuman => ActivityOutcome::NeedsHuman,
                    Outcome::Failed => ActivityOutcome::Failed,
                };
                Event::ActivityEnded { activity, outcome }
            }
            Action::Abandon(index) => {
                if self.activities.is_empty() {
                    return None;
                }
                let activity = self.activities.remove(index.index(self.activities.len()));
                Event::ActivityAbandoned { activity }
            }
            Action::Checks(index, green, on_head) => {
                let run = self.pick_run(index)?;
                Event::ChecksConcluded {
                    run,
                    head: self.head_or_other(run, *on_head),
                    conclusion: if *green {
                        CiConclusion::Green
                    } else {
                        CiConclusion::Red
                    },
                }
            }
            Action::Review(index, choice) => {
                let run = self.pick_run(index)?;
                let reviewer = Actor {
                    login: "reviewer".to_owned(),
                    kind: choice.kind,
                    write_permission: choice.write_permission,
                };
                let commit = self.head_or_other(run, choice.on_head);
                if choice.verdict == ReviewVerdict::Approved && reviewer.is_authorised() {
                    self.approved.entry(run).or_default().insert(commit.clone());
                }
                Event::ReviewSubmitted {
                    run,
                    review: ReviewId(u64::from(choice.id)),
                    reviewer,
                    commit,
                    verdict: choice.verdict,
                }
            }
            Action::Merged(index) => Event::PullRequestMerged {
                run: self.pick_run(index)?,
            },
            Action::Cancel(index) => Event::CancelObserved {
                run: self.pick_run(index)?,
                cause: CancelCause::IssueClosed,
            },
            Action::Redeliver(index) => {
                if self.delivered.is_empty() {
                    return None;
                }
                self.delivered[index.index(self.delivered.len())].clone()
            }
        })
    }

    /// The last SHA pushed for the run, or one that it never had.
    fn head_or_other(&self, run: RunId, on_head: bool) -> Sha {
        match self.pushes.get(&run).and_then(|pushes| pushes.last()) {
            Some(head) if on_head => head.clone(),
            _ => Sha("never-pushed".to_owned()),
        }
    }

    /// Steps the engine and checks the properties that hold after any step.
    fn apply(&mut self, action: &Action) -> Result<(), TestCaseError> {
        let Some(event) = self.event(action) else {
            return Ok(());
        };
        let before = self.state.clone();
        let (state, effects) = step(before.clone(), event.clone());

        // ORCH-1: the same state and event give the same answer.
        prop_assert_eq!(
            step(before.clone(), event.clone()),
            (state.clone(), effects.clone())
        );

        // A result event that was delivered before leaves the state as it is
        // and asks for nothing.
        if matches!(action, Action::Redeliver(_)) {
            prop_assert_eq!(&state, &before);
            prop_assert!(effects.is_empty());
        }
        if matches!(
            event,
            Event::WriteCompleted { .. }
                | Event::ActivityEnded { .. }
                | Event::ActivityAbandoned { .. }
        ) {
            self.delivered.push(event.clone());
            // Delivered twice in a row, it is the same.
            let again = step(state.clone(), event.clone());
            prop_assert_eq!(&again.0, &state);
            prop_assert!(again.1.is_empty());
        }
        if let Event::ActivityEnded { activity, .. } | Event::ActivityAbandoned { activity } = event
            && self.live.get(&activity.run) == Some(&activity)
        {
            if state.runs[&activity.run].activity == Some(activity) {
                // An outcome that the stage cannot have was ignored, and the
                // activity is live still: it may end again.
                self.activities.push(activity);
            } else {
                self.live.remove(&activity.run);
            }
        }

        for effect in &effects {
            match effect {
                Effect::Write { id, write } => {
                    if let Write::Merge { head } = write {
                        self.check_merge(id.run, head)?;
                    }
                    self.writes.push((*id, write.clone()));
                }
                Effect::StartActivity { id, stage } => {
                    self.stages.insert(*id, *stage);
                    // INV-6: at most one stage execution per run at a time.
                    prop_assert!(
                        !self.live.contains_key(&id.run),
                        "a second activity started for {:?}",
                        id.run
                    );
                    self.live.insert(id.run, *id);
                    self.activities.push(*id);
                }
                Effect::InterruptActivity { id } => {
                    prop_assert_eq!(self.live.remove(&id.run), Some(*id));
                }
            }
        }
        self.state = state;
        self.check_state()
    }

    /// No merge effect without a valid approval (PR-2, PR-4): the head is the
    /// SHA the factory last pushed, and an authorised User approved it after
    /// that push.
    fn check_merge(&self, run: RunId, head: &Sha) -> Result<(), TestCaseError> {
        prop_assert_eq!(
            self.state.config.merge_strategy,
            MergeStrategy::FactorySerial
        );
        let last = self.pushes.get(&run).and_then(|pushes| pushes.last());
        prop_assert_eq!(last, Some(head), "merge of a head not last pushed");
        prop_assert!(
            self.approved
                .get(&run)
                .is_some_and(|approved| approved.contains(head)),
            "merge of {:?} without a valid approval",
            head
        );
        Ok(())
    }

    fn check_state(&self) -> Result<(), TestCaseError> {
        let caps = self.state.config.caps;
        let mut active = BTreeSet::new();
        for run in self.state.runs.values() {
            // INV-6: at most one non-terminal run per issue.
            if !run.is_terminal() {
                prop_assert!(
                    active.insert(run.issue),
                    "two active runs of {:?}",
                    run.issue
                );
            }
            // The round caps of 7.3.
            prop_assert!(run.rounds.verify <= caps.verify);
            prop_assert!(run.rounds.ci <= caps.ci);
            prop_assert!(run.rounds.review <= caps.review);
            prop_assert!(run.rounds.human <= caps.human);
            prop_assert!(run.rounds.plan <= caps.plan);
            prop_assert!(run.rounds.merge_conflict <= caps.merge_conflict);
            // The live activity is the one the effects say.
            prop_assert_eq!(run.activity, self.live.get(&run.id).copied());
            // An approval is on the head (PR-2).
            if let Some(approval) = &run.approval {
                prop_assert_eq!(Some(&approval.commit), run.head.as_ref());
            }
            prop_assert!(run.stage != Stage::Merge || run.approval.is_some());
        }
        Ok(())
    }
}

fn strategy() -> impl Strategy<Value = MergeStrategy> {
    prop_oneof![
        Just(MergeStrategy::Human),
        Just(MergeStrategy::FactorySerial)
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Every invariant of the row "Engine" of 19.4 holds after every step of
    /// a sequence.
    #[test]
    fn the_invariants_hold_over_event_sequences(
        merge_strategy in strategy(),
        actions in prop::collection::vec(action(), 1..300),
    ) {
        let mut driver = Driver::new(merge_strategy);
        for action in &actions {
            driver.apply(action)?;
        }
    }

    /// A result event delivered again later, after other events, leaves the
    /// state unchanged wherever in the sequence it comes.
    #[test]
    fn a_result_delivered_again_later_changes_nothing(
        merge_strategy in strategy(),
        actions in prop::collection::vec(action(), 1..200),
        picks in prop::collection::vec(any::<Index>(), 1..20),
    ) {
        let mut driver = Driver::new(merge_strategy);
        for action in &actions {
            driver.apply(action)?;
        }
        for pick in &picks {
            driver.apply(&Action::Redeliver(*pick))?;
        }
    }
}

/// The generated sequences reach the end of the pipeline: without it the
/// merge property would hold for want of a merge.
#[test]
fn generated_sequences_reach_a_merge() {
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;

    let mut runner = TestRunner::deterministic();
    let sequences = prop::collection::vec(action(), 300);
    let mut merges = 0;
    for _ in 0..200 {
        let actions = sequences
            .new_tree(&mut runner)
            .expect("a sequence")
            .current();
        let mut driver = Driver::new(MergeStrategy::FactorySerial);
        for action in &actions {
            driver.apply(action).expect("the properties hold");
        }
        merges += driver
            .state
            .runs
            .values()
            .filter(|run| run.stage == Stage::Done && run.approval.is_some())
            .count();
    }
    // 80 runs merged in the 200 sequences when this was written.
    assert!(
        merges >= 20,
        "{merges} runs merged in 200 generated sequences"
    );
}
