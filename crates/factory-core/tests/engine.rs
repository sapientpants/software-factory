//! The engine skeleton of SPEC 8.1, by example: each transition that it
//! makes, and each event that it leaves without effect (19.4).

use factory_core::engine::{
    ActivityId, ActivityOutcome, Actor, ActorKind, CancelCause, CiConclusion, Effect, EffectId,
    EngineConfig, EscalationCategory, Event, IssueKey, MergeStrategy, PublicState, RepoId,
    ReviewId, ReviewVerdict, RoundCaps, Run, RunId, RunStatus, Sha, Stage, State, Timestamp, Write,
    WriteOutcome, step,
};

const RUN: RunId = RunId(7);
const ISSUE: IssueKey = IssueKey {
    repo: RepoId(1),
    number: 42,
};

fn sha(name: &str) -> Sha {
    Sha(name.to_owned())
}

fn operator() -> Actor {
    Actor {
        login: "operator".to_owned(),
        kind: ActorKind::User,
        write_permission: true,
    }
}

/// One state, the effects of its last step and every effect so far.
struct Harness {
    state: State,
    last: Vec<Effect>,
    reviews: u64,
}

impl Harness {
    fn new(merge_strategy: MergeStrategy) -> Self {
        Self {
            state: State::new(EngineConfig {
                caps: RoundCaps::default(),
                merge_strategy,
            }),
            last: Vec::new(),
            reviews: 0,
        }
    }

    /// A harness whose run is claimed and whose claim writes succeeded.
    fn planning(merge_strategy: MergeStrategy) -> Self {
        let mut h = Self::new(merge_strategy);
        h.claim();
        h.complete_writes();
        h
    }

    fn send(&mut self, event: Event) -> &[Effect] {
        let (state, effects) = step(self.state.clone(), event);
        self.state = state;
        self.last = effects;
        &self.last
    }

    /// Sends an event and asserts that it changes nothing.
    fn ignored(&mut self, event: Event) {
        let before = self.state.clone();
        assert!(self.send(event).is_empty());
        assert_eq!(self.state, before);
    }

    fn run(&self) -> &Run {
        &self.state.runs[&RUN]
    }

    fn claim(&mut self) -> &[Effect] {
        self.send(Event::Claim {
            run: RUN,
            issue: ISSUE,
            base_sha: sha("base"),
            at: Timestamp(1_000),
        })
    }

    /// Completes every awaited write with success, in order.
    fn complete_writes(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        for effect in self.run().awaiting.clone() {
            effects.extend(
                self.send(Event::WriteCompleted {
                    effect,
                    outcome: WriteOutcome::Succeeded,
                })
                .to_vec(),
            );
        }
        effects
    }

    fn activity(&self) -> ActivityId {
        let Some(activity) = self.run().activity else {
            panic!("no live activity");
        };
        activity
    }

    fn end(&mut self, outcome: ActivityOutcome) -> &[Effect] {
        let activity = self.activity();
        self.send(Event::ActivityEnded { activity, outcome })
    }

    fn checks(&mut self, conclusion: CiConclusion) -> &[Effect] {
        let head = self.head();
        self.send(Event::ChecksConcluded {
            run: RUN,
            head,
            conclusion,
        })
    }

    fn review(&mut self, reviewer: Actor, commit: Sha, verdict: ReviewVerdict) -> &[Effect] {
        self.reviews += 1;
        let review = ReviewId(self.reviews);
        self.send(Event::ReviewSubmitted {
            run: RUN,
            review,
            reviewer,
            commit,
            verdict,
        })
    }

    fn head(&self) -> Sha {
        let Some(head) = self.run().head.clone() else {
            panic!("no head");
        };
        head
    }

    /// From `implement` to `ci`, with this candidate.
    fn reach_ci(&mut self, candidate: &str) {
        assert_eq!(self.run().stage, Stage::Implement);
        self.end(ActivityOutcome::Succeeded);
        self.end(ActivityOutcome::Captured(sha(candidate)));
        self.end(ActivityOutcome::Succeeded);
        self.complete_writes();
        assert_eq!(self.run().stage, Stage::Ci);
    }

    /// From `ci` to `human_review`.
    fn reach_human_review(&mut self) {
        self.checks(CiConclusion::Green);
        self.end(ActivityOutcome::Succeeded);
        self.complete_writes();
        assert_eq!(self.run().stage, Stage::HumanReview);
    }

    /// A harness whose run waits in `human_review` on head `c1`.
    fn in_human_review(merge_strategy: MergeStrategy) -> Self {
        let mut h = Self::planning(merge_strategy);
        h.end(ActivityOutcome::Succeeded);
        h.reach_ci("c1");
        h.reach_human_review();
        h
    }

    fn assert_escalated(&self, category: EscalationCategory) {
        assert_eq!(self.run().escalation, Some(category));
        assert_eq!(self.run().status, RunStatus::Active);
        assert_eq!(self.run().activity, None);
        assert!(self.run().awaiting.is_empty());
        assert_eq!(
            writes(&self.last),
            [
                Write::SetState(PublicState::NeedsHuman),
                Write::PostEscalation(category)
            ]
        );
    }
}

fn writes(effects: &[Effect]) -> Vec<Write> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Write { write, .. } => Some(write.clone()),
            _ => None,
        })
        .collect()
}

fn started(effects: &[Effect]) -> Vec<Stage> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::StartActivity { stage, .. } => Some(*stage),
            _ => None,
        })
        .collect()
}

#[test]
fn the_caps_are_those_of_7_3() {
    let caps = RoundCaps::default();
    assert_eq!(
        (
            caps.plan,
            caps.verify,
            caps.ci,
            caps.review,
            caps.human,
            caps.merge_conflict
        ),
        (2, 3, 2, 2, 3, 2)
    );
}

#[test]
fn an_authorised_actor_is_a_user_with_write_permission() {
    let cases = [
        (ActorKind::User, true, true),
        (ActorKind::User, false, false),
        (ActorKind::Bot, true, false),
        (ActorKind::Bot, false, false),
    ];
    for (kind, write_permission, authorised) in cases {
        let actor = Actor {
            login: "a".to_owned(),
            kind,
            write_permission,
        };
        assert_eq!(
            actor.is_authorised(),
            authorised,
            "{kind:?} {write_permission}"
        );
    }
}

#[test]
fn a_claim_creates_the_run_and_awaits_its_three_writes() {
    let mut h = Harness::new(MergeStrategy::Human);
    let effects = h.claim().to_vec();
    assert_eq!(
        effects,
        [
            Effect::Write {
                id: EffectId { run: RUN, seq: 0 },
                write: Write::CreateBranch {
                    base_sha: sha("base")
                }
            },
            Effect::Write {
                id: EffectId { run: RUN, seq: 1 },
                write: Write::SetState(PublicState::InProgress)
            },
            Effect::Write {
                id: EffectId { run: RUN, seq: 2 },
                write: Write::CreateStatusComment
            },
        ]
    );
    let run = h.run();
    assert_eq!(run.issue, ISSUE);
    assert_eq!(run.claimed_at, Timestamp(1_000));
    assert_eq!(run.base_sha, sha("base"));
    assert_eq!(run.attempt, 1);
    assert_eq!(run.stage, Stage::Claim);
    assert_eq!(run.status, RunStatus::Active);
    assert_eq!(run.awaiting.len(), 3);
    assert_eq!(run.next_seq, 3);
    assert_eq!(h.state.active_run(ISSUE).map(|run| run.id), Some(RUN));
}

#[test]
fn the_claim_ends_when_its_last_write_succeeds() {
    let mut h = Harness::new(MergeStrategy::Human);
    h.claim();
    let ids: Vec<EffectId> = h.run().awaiting.iter().copied().collect();
    for id in &ids[..2] {
        assert!(
            h.send(Event::WriteCompleted {
                effect: *id,
                outcome: WriteOutcome::Succeeded
            })
            .is_empty()
        );
        assert_eq!(h.run().stage, Stage::Claim);
    }
    let effects = h
        .send(Event::WriteCompleted {
            effect: ids[2],
            outcome: WriteOutcome::Succeeded,
        })
        .to_vec();
    assert_eq!(
        effects,
        [Effect::StartActivity {
            id: ActivityId { run: RUN, seq: 3 },
            stage: Stage::Plan
        }]
    );
    assert_eq!(h.run().stage, Stage::Plan);
}

#[test]
fn a_second_claim_of_an_issue_with_an_active_run_is_ignored() {
    let mut h = Harness::planning(MergeStrategy::Human);
    h.ignored(Event::Claim {
        run: RunId(8),
        issue: ISSUE,
        base_sha: sha("other"),
        at: Timestamp(2_000),
    });
    // The same run id, even for another issue.
    h.ignored(Event::Claim {
        run: RUN,
        issue: IssueKey {
            repo: RepoId(1),
            number: 43,
        },
        base_sha: sha("other"),
        at: Timestamp(2_000),
    });
}

#[test]
fn an_escalated_run_is_active_and_blocks_a_claim() {
    let mut h = Harness::planning(MergeStrategy::Human);
    h.end(ActivityOutcome::NeedsHuman);
    h.ignored(Event::Claim {
        run: RunId(8),
        issue: ISSUE,
        base_sha: sha("other"),
        at: Timestamp(2_000),
    });
}

#[test]
fn a_claim_of_another_issue_or_after_a_terminal_run_creates_a_run() {
    let mut h = Harness::planning(MergeStrategy::Human);
    let other = IssueKey {
        repo: RepoId(2),
        number: 42,
    };
    h.send(Event::Claim {
        run: RunId(8),
        issue: other,
        base_sha: sha("b"),
        at: Timestamp(2_000),
    });
    assert_eq!(h.state.active_run(other).map(|run| run.id), Some(RunId(8)));
    h.send(Event::CancelObserved {
        run: RUN,
        cause: CancelCause::TriageApplied,
    });
    assert!(h.state.active_run(ISSUE).is_none());
    h.send(Event::Claim {
        run: RunId(9),
        issue: ISSUE,
        base_sha: sha("b"),
        at: Timestamp(3_000),
    });
    assert_eq!(h.state.active_run(ISSUE).map(|run| run.id), Some(RunId(9)));
    assert_eq!(h.state.runs.len(), 3);
}

#[test]
fn the_pipeline_runs_to_human_review_and_the_human_merges() {
    let mut h = Harness::planning(MergeStrategy::Human);
    assert_eq!(
        started(h.end(ActivityOutcome::Succeeded)),
        [Stage::Implement]
    );
    assert_eq!(started(h.end(ActivityOutcome::Succeeded)), [Stage::Capture]);
    assert_eq!(
        started(h.end(ActivityOutcome::Captured(sha("c1")))),
        [Stage::Verify]
    );
    assert_eq!(h.run().candidate, Some(sha("c1")));
    let publish = h.end(ActivityOutcome::Succeeded).to_vec();
    assert_eq!(
        writes(&publish),
        [
            Write::Push {
                candidate: sha("c1")
            },
            Write::UpsertPullRequest
        ]
    );
    assert_eq!(h.run().stage, Stage::Publish);
    assert!(h.complete_writes().is_empty());
    assert_eq!(h.run().stage, Stage::Ci);
    assert_eq!(h.run().head, Some(sha("c1")));
    assert!(h.run().pushed.contains(&sha("c1")));
    assert!(h.run().pull_request);
    assert_eq!(started(h.checks(CiConclusion::Green)), [Stage::Review]);
    let ready = h.end(ActivityOutcome::Succeeded).to_vec();
    assert_eq!(
        writes(&ready),
        [Write::MarkReady, Write::SetState(PublicState::InReview)]
    );
    assert_eq!(h.run().stage, Stage::Ready);
    assert!(h.complete_writes().is_empty());
    assert_eq!(h.run().stage, Stage::HumanReview);

    // Under `human` a valid approval is recorded and merges nothing.
    assert!(
        h.review(operator(), sha("c1"), ReviewVerdict::Approved)
            .is_empty()
    );
    let approval = h.run().approval.clone().expect("an approval");
    assert_eq!(
        (approval.by.as_str(), approval.commit),
        ("operator", sha("c1"))
    );
    assert_eq!(h.run().stage, Stage::HumanReview);

    let done = h.send(Event::PullRequestMerged { run: RUN }).to_vec();
    assert_eq!(writes(&done), [Write::UpdateStatusComment]);
    assert_eq!(h.run().stage, Stage::Done);
    assert_eq!(h.run().status, RunStatus::Done);
    assert!(h.run().is_terminal());
}

#[test]
fn under_factory_serial_a_valid_approval_merges_the_head() {
    let mut h = Harness::in_human_review(MergeStrategy::FactorySerial);
    let effects = h
        .review(operator(), sha("c1"), ReviewVerdict::Approved)
        .to_vec();
    assert_eq!(writes(&effects), [Write::Merge { head: sha("c1") }]);
    assert_eq!(h.run().stage, Stage::Merge);
    let done = h.complete_writes();
    assert_eq!(writes(&done), [Write::UpdateStatusComment]);
    assert_eq!(h.run().status, RunStatus::Done);
}

#[test]
fn an_approval_that_does_not_count_merges_nothing() {
    let bot = Actor {
        kind: ActorKind::Bot,
        ..operator()
    };
    let reader = Actor {
        write_permission: false,
        ..operator()
    };
    let cases = [
        (bot, sha("c1"), ReviewVerdict::Approved),
        (reader, sha("c1"), ReviewVerdict::Approved),
        (operator(), sha("c0"), ReviewVerdict::Approved),
        (operator(), sha("c1"), ReviewVerdict::Commented),
    ];
    for (reviewer, commit, verdict) in cases {
        let mut h = Harness::in_human_review(MergeStrategy::FactorySerial);
        assert!(h.review(reviewer, commit, verdict).is_empty());
        assert_eq!(h.run().stage, Stage::HumanReview);
        assert_eq!(h.run().approval, None);
    }
}

#[test]
fn a_review_outside_human_review_is_ignored() {
    let mut h = Harness::planning(MergeStrategy::FactorySerial);
    h.end(ActivityOutcome::Succeeded);
    h.reach_ci("c1");
    h.ignored(Event::ReviewSubmitted {
        run: RUN,
        review: ReviewId(1),
        reviewer: operator(),
        commit: sha("c1"),
        verdict: ReviewVerdict::Approved,
    });
}

#[test]
fn a_review_starts_one_round_and_the_push_after_it_voids_the_approval() {
    let mut h = Harness::in_human_review(MergeStrategy::Human);
    h.review(operator(), sha("c1"), ReviewVerdict::Approved);
    assert!(h.run().approval.is_some());
    let event = Event::ReviewSubmitted {
        run: RUN,
        review: ReviewId(100),
        reviewer: operator(),
        commit: sha("c1"),
        verdict: ReviewVerdict::ChangesRequested,
    };
    let effects = h.send(event.clone()).to_vec();
    assert_eq!(writes(&effects), [Write::SetState(PublicState::InProgress)]);
    assert_eq!(started(&effects), [Stage::Implement]);
    assert_eq!(h.run().rounds.human, 1);
    assert_eq!(h.run().candidate, None);
    h.reach_ci("c2");
    assert_eq!(h.run().approval, None);
    assert_eq!(h.head(), sha("c2"));
    assert!(h.run().pushed.contains(&sha("c1")));
    h.reach_human_review();
    // The same review again starts no second round.
    h.ignored(event);
    assert_eq!(h.run().rounds.human, 1);
}

#[test]
fn verify_rounds_are_capped_at_three() {
    let mut h = Harness::planning(MergeStrategy::Human);
    h.end(ActivityOutcome::Succeeded);
    for round in 1..=3 {
        h.end(ActivityOutcome::Succeeded);
        if round == 2 {
            // A rejected capture is a verify round too.
            assert_eq!(h.run().stage, Stage::Capture);
            let effects = h.end(ActivityOutcome::GateFailed).to_vec();
            assert!(writes(&effects).is_empty());
            assert_eq!(started(&effects), [Stage::Implement]);
        } else {
            h.end(ActivityOutcome::Captured(sha("c")));
            assert_eq!(h.run().stage, Stage::Verify);
            let effects = h.end(ActivityOutcome::GateFailed).to_vec();
            assert_eq!(started(&effects), [Stage::Implement]);
        }
        assert_eq!(h.run().rounds.verify, round);
        assert_eq!(h.run().candidate, None);
    }
    h.end(ActivityOutcome::Succeeded);
    h.end(ActivityOutcome::Captured(sha("c")));
    h.end(ActivityOutcome::GateFailed);
    h.assert_escalated(EscalationCategory::Verification);
    assert_eq!(h.run().rounds.verify, 3);
    assert_eq!(h.run().stage, Stage::Verify);
}

#[test]
fn ci_rounds_are_capped_at_two() {
    let mut h = Harness::planning(MergeStrategy::Human);
    h.end(ActivityOutcome::Succeeded);
    for round in 1..=2 {
        h.reach_ci("c");
        let effects = h.checks(CiConclusion::Red).to_vec();
        assert_eq!(started(&effects), [Stage::Implement]);
        assert!(writes(&effects).is_empty());
        assert_eq!(h.run().rounds.ci, round);
    }
    h.reach_ci("c");
    h.checks(CiConclusion::Red);
    h.assert_escalated(EscalationCategory::Verification);
    assert_eq!(h.run().rounds.ci, 2);
}

#[test]
fn review_rounds_are_capped_at_two() {
    let mut h = Harness::planning(MergeStrategy::Human);
    h.end(ActivityOutcome::Succeeded);
    for round in 1..=2 {
        h.reach_ci("c");
        h.checks(CiConclusion::Green);
        let effects = h.end(ActivityOutcome::GateFailed).to_vec();
        assert_eq!(started(&effects), [Stage::Implement]);
        assert_eq!(h.run().rounds.review, round);
    }
    h.reach_ci("c");
    h.checks(CiConclusion::Green);
    h.end(ActivityOutcome::GateFailed);
    h.assert_escalated(EscalationCategory::Review);
    assert_eq!(h.run().rounds.review, 2);
}

#[test]
fn human_rounds_are_capped_at_three() {
    let mut h = Harness::in_human_review(MergeStrategy::Human);
    for round in 1..=3 {
        let head = h.head();
        h.review(operator(), head, ReviewVerdict::ChangesRequested);
        assert_eq!(h.run().rounds.human, round);
        h.reach_ci("c");
        h.reach_human_review();
    }
    let head = h.head();
    h.review(operator(), head, ReviewVerdict::ChangesRequested);
    h.assert_escalated(EscalationCategory::Review);
    assert_eq!(h.run().rounds.human, 3);
}

#[test]
fn changes_requested_by_someone_unauthorised_start_no_round() {
    let mut h = Harness::in_human_review(MergeStrategy::Human);
    let reader = Actor {
        write_permission: false,
        ..operator()
    };
    assert!(
        h.review(reader, sha("c1"), ReviewVerdict::ChangesRequested)
            .is_empty()
    );
    assert_eq!(h.run().rounds.human, 0);
    assert_eq!(h.run().stage, Stage::HumanReview);
}

#[test]
fn an_agent_failure_restarts_the_stage_once_with_a_new_id() {
    let mut h = Harness::planning(MergeStrategy::Human);
    let first = h.activity();
    let effects = h.end(ActivityOutcome::Failed).to_vec();
    let second = h.activity();
    assert_ne!(first, second);
    assert_eq!(
        effects,
        [Effect::StartActivity {
            id: second,
            stage: Stage::Plan
        }]
    );
    assert_eq!(h.run().restarts, 1);
    // The restart is counted per stage: implement may restart once too.
    h.end(ActivityOutcome::Succeeded);
    assert_eq!(h.run().restarts, 0);
    assert_eq!(started(h.end(ActivityOutcome::Failed)), [Stage::Implement]);
    h.end(ActivityOutcome::Failed);
    h.assert_escalated(EscalationCategory::Verification);
    assert_eq!(h.run().stage, Stage::Implement);
}

#[test]
fn a_reviewer_failure_restarts_and_then_escalates() {
    let mut h = Harness::planning(MergeStrategy::Human);
    h.end(ActivityOutcome::Succeeded);
    h.reach_ci("c1");
    h.checks(CiConclusion::Green);
    assert_eq!(started(h.end(ActivityOutcome::Failed)), [Stage::Review]);
    h.end(ActivityOutcome::Failed);
    h.assert_escalated(EscalationCategory::Verification);
}

#[test]
fn needs_human_from_an_agent_escalates_with_clarification() {
    for stage in [Stage::Plan, Stage::Implement] {
        let mut h = Harness::planning(MergeStrategy::Human);
        if stage == Stage::Implement {
            h.end(ActivityOutcome::Succeeded);
        }
        h.end(ActivityOutcome::NeedsHuman);
        h.assert_escalated(EscalationCategory::Clarification);
        assert_eq!(h.run().stage, stage);
    }
}

#[test]
fn a_capture_or_verify_that_cannot_run_escalates_with_infrastructure() {
    for stage in [Stage::Capture, Stage::Verify] {
        let mut h = Harness::planning(MergeStrategy::Human);
        h.end(ActivityOutcome::Succeeded);
        h.end(ActivityOutcome::Succeeded);
        if stage == Stage::Verify {
            h.end(ActivityOutcome::Captured(sha("c1")));
        }
        assert_eq!(h.run().stage, stage);
        h.end(ActivityOutcome::Failed);
        h.assert_escalated(EscalationCategory::Infrastructure);
    }
}

#[test]
fn an_outcome_that_the_stage_cannot_have_is_ignored() {
    let mut h = Harness::planning(MergeStrategy::Human);
    let activity = h.activity();
    for outcome in [
        ActivityOutcome::Captured(sha("c")),
        ActivityOutcome::GateFailed,
    ] {
        h.ignored(Event::ActivityEnded { activity, outcome });
    }
    h.end(ActivityOutcome::Succeeded);
    h.end(ActivityOutcome::Succeeded);
    let activity = h.activity();
    for outcome in [ActivityOutcome::Succeeded, ActivityOutcome::NeedsHuman] {
        h.ignored(Event::ActivityEnded { activity, outcome });
    }
}

#[test]
fn the_third_abandoned_activity_of_a_stage_escalates() {
    let mut h = Harness::planning(MergeStrategy::Human);
    for abandoned in 1..=2 {
        let activity = h.activity();
        let effects = h.send(Event::ActivityAbandoned { activity }).to_vec();
        assert_eq!(started(&effects), [Stage::Plan]);
        assert_ne!(h.activity(), activity);
        assert_eq!(h.run().abandoned, abandoned);
    }
    // A new stage counts anew.
    h.end(ActivityOutcome::Succeeded);
    assert_eq!(h.run().abandoned, 0);
    for _ in 1..=2 {
        let activity = h.activity();
        h.send(Event::ActivityAbandoned { activity });
    }
    let activity = h.activity();
    h.send(Event::ActivityAbandoned { activity });
    h.assert_escalated(EscalationCategory::Infrastructure);
    assert_eq!(h.run().abandoned, 3);
}

#[test]
fn a_failed_write_escalates_with_infrastructure() {
    let mut h = Harness::new(MergeStrategy::Human);
    h.claim();
    let first = *h.run().awaiting.first().expect("a write");
    h.send(Event::WriteCompleted {
        effect: first,
        outcome: WriteOutcome::Failed,
    });
    h.assert_escalated(EscalationCategory::Infrastructure);
    assert_eq!(h.run().stage, Stage::Claim);
}

#[test]
fn an_escalated_run_ignores_its_pipeline_events() {
    let mut h = Harness::in_human_review(MergeStrategy::FactorySerial);
    let head = h.head();
    // Exceed the human cap to escalate in `human_review`.
    let caps = RoundCaps {
        human: 0,
        ..RoundCaps::default()
    };
    h.state.config.caps = caps;
    h.review(operator(), head.clone(), ReviewVerdict::ChangesRequested);
    h.assert_escalated(EscalationCategory::Review);
    h.ignored(Event::ReviewSubmitted {
        run: RUN,
        review: ReviewId(999),
        reviewer: operator(),
        commit: head.clone(),
        verdict: ReviewVerdict::Approved,
    });
    h.ignored(Event::ChecksConcluded {
        run: RUN,
        head,
        conclusion: CiConclusion::Red,
    });
    h.ignored(Event::WriteCompleted {
        effect: EffectId { run: RUN, seq: 0 },
        outcome: WriteOutcome::Succeeded,
    });
}

#[test]
fn an_escalated_run_takes_its_merge() {
    let mut h = Harness::planning(MergeStrategy::Human);
    h.end(ActivityOutcome::Succeeded);
    h.reach_ci("c1");
    h.checks(CiConclusion::Green);
    h.end(ActivityOutcome::NeedsHuman);
    let effects = h.send(Event::PullRequestMerged { run: RUN }).to_vec();
    assert_eq!(writes(&effects), [Write::UpdateStatusComment]);
    assert_eq!(h.run().escalation, None);
    assert_eq!(h.run().status, RunStatus::Done);
}

#[test]
fn a_merge_interrupts_a_live_activity() {
    let mut h = Harness::planning(MergeStrategy::Human);
    h.end(ActivityOutcome::Succeeded);
    h.reach_ci("c1");
    h.checks(CiConclusion::Green);
    let activity = h.activity();
    let effects = h.send(Event::PullRequestMerged { run: RUN }).to_vec();
    assert_eq!(effects[0], Effect::InterruptActivity { id: activity });
    assert_eq!(h.run().activity, None);
    assert_eq!(h.run().stage, Stage::Done);
}

#[test]
fn a_merge_clears_the_awaited_writes_of_the_stage() {
    let mut h = Harness::in_human_review(MergeStrategy::Human);
    let head = h.head();
    h.review(operator(), head, ReviewVerdict::ChangesRequested);
    h.end(ActivityOutcome::Succeeded);
    h.end(ActivityOutcome::Captured(sha("c2")));
    h.end(ActivityOutcome::Succeeded);
    assert_eq!(h.run().stage, Stage::Publish);
    h.send(Event::PullRequestMerged { run: RUN });
    assert!(h.run().awaiting.is_empty());
    assert_eq!(h.run().status, RunStatus::Done);
}

#[test]
fn a_merge_without_a_pull_request_is_ignored() {
    let mut h = Harness::planning(MergeStrategy::Human);
    h.ignored(Event::PullRequestMerged { run: RUN });
}

#[test]
fn a_cancel_interrupts_the_activity_and_closes_the_pull_request() {
    let mut h = Harness::planning(MergeStrategy::Human);
    h.end(ActivityOutcome::Succeeded);
    h.reach_ci("c1");
    h.checks(CiConclusion::Green);
    let activity = h.activity();
    let effects = h
        .send(Event::CancelObserved {
            run: RUN,
            cause: CancelCause::IssueClosed,
        })
        .to_vec();
    assert_eq!(effects[0], Effect::InterruptActivity { id: activity });
    assert_eq!(writes(&effects), [Write::ClosePullRequest]);
    assert_eq!(h.run().status, RunStatus::Cancelled);
    assert_eq!(h.run().activity, None);
}

#[test]
fn a_cancel_before_a_pull_request_closes_none() {
    let mut h = Harness::new(MergeStrategy::Human);
    h.claim();
    assert!(
        h.send(Event::CancelObserved {
            run: RUN,
            cause: CancelCause::TriageApplied,
        })
        .is_empty()
    );
    assert_eq!(h.run().status, RunStatus::Cancelled);
    assert!(h.run().awaiting.is_empty());
}

#[test]
fn an_escalated_run_can_be_cancelled() {
    let mut h = Harness::planning(MergeStrategy::Human);
    h.end(ActivityOutcome::NeedsHuman);
    h.send(Event::CancelObserved {
        run: RUN,
        cause: CancelCause::TriageApplied,
    });
    assert_eq!(h.run().status, RunStatus::Cancelled);
}

#[test]
fn a_terminal_run_ignores_every_event() {
    let mut h = Harness::in_human_review(MergeStrategy::Human);
    h.send(Event::PullRequestMerged { run: RUN });
    let head = h.head();
    h.ignored(Event::PullRequestMerged { run: RUN });
    h.ignored(Event::CancelObserved {
        run: RUN,
        cause: CancelCause::IssueClosed,
    });
    h.ignored(Event::ReviewSubmitted {
        run: RUN,
        review: ReviewId(50),
        reviewer: operator(),
        commit: head,
        verdict: ReviewVerdict::ChangesRequested,
    });
}

#[test]
fn stale_and_unknown_events_are_ignored() {
    let mut h = Harness::planning(MergeStrategy::Human);
    let old = h.activity();
    h.end(ActivityOutcome::Failed);
    // The activity that the restart replaced.
    h.ignored(Event::ActivityEnded {
        activity: old,
        outcome: ActivityOutcome::Succeeded,
    });
    h.ignored(Event::ActivityAbandoned { activity: old });
    h.ignored(Event::ActivityStarted {
        activity: h.activity(),
    });
    // A write that was applied already.
    h.ignored(Event::WriteCompleted {
        effect: EffectId { run: RUN, seq: 0 },
        outcome: WriteOutcome::Failed,
    });
    // Checks outside `ci`.
    h.ignored(Event::ChecksConcluded {
        run: RUN,
        head: sha("base"),
        conclusion: CiConclusion::Red,
    });
    // Events of a run that does not exist.
    let ghost = RunId(99);
    for event in [
        Event::WriteCompleted {
            effect: EffectId { run: ghost, seq: 0 },
            outcome: WriteOutcome::Succeeded,
        },
        Event::ActivityEnded {
            activity: ActivityId { run: ghost, seq: 0 },
            outcome: ActivityOutcome::Succeeded,
        },
        Event::ActivityAbandoned {
            activity: ActivityId { run: ghost, seq: 0 },
        },
        Event::ChecksConcluded {
            run: ghost,
            head: sha("c"),
            conclusion: CiConclusion::Green,
        },
        Event::ReviewSubmitted {
            run: ghost,
            review: ReviewId(1),
            reviewer: operator(),
            commit: sha("c"),
            verdict: ReviewVerdict::Approved,
        },
        Event::PullRequestMerged { run: ghost },
        Event::CancelObserved {
            run: ghost,
            cause: CancelCause::IssueClosed,
        },
    ] {
        h.ignored(event);
    }
}

#[test]
fn checks_on_a_head_that_is_not_the_runs_are_ignored() {
    let mut h = Harness::planning(MergeStrategy::Human);
    h.end(ActivityOutcome::Succeeded);
    h.reach_ci("c1");
    h.ignored(Event::ChecksConcluded {
        run: RUN,
        head: sha("c0"),
        conclusion: CiConclusion::Red,
    });
}
