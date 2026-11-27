//! The pure engine of SPEC 8.1, as the skeleton that M0 builds (22).
//!
//! [`step`] is `step(State, Event) -> (State, Vec<Effect>)` (ORCH-1). It reads
//! no clock, makes no id at random and performs no I/O: the time and the run
//! id of a claim arrive in its [`Event`], made by the caller through [`Clock`]
//! and [`IdGen`], and every other id is derived from the state. The effect id
//! and the activity id are the run id and a sequence number of the run, as
//! the `<run>/<seq>` of the comment marker of 8.5 is. The maps of [`State`]
//! are ordered, so the effects of a step come in one order on every run.
//!
//! The skeleton drives the stages `claim`, `plan`, `implement`, `capture`,
//! `verify`, `publish`, `ci`, `review`, `ready`, `human_review`, `merge` and
//! `done` of 7.3, with the round caps of 7.3, the approval of PR-2 and the
//! cancel and merge rows of 7.7. [`Stage`] names every stage of 7.3 and 7.4,
//! and the engine enters only those it drives. The writes of `claim`,
//! `publish`, `ready` and `merge` are awaited before the run leaves the stage
//! (ORCH-7). Those of `done`, of a cancel, of an escalation and the label of a
//! human round are not yet: their results are dropped. A result is applied
//! once (ORCH-3): a result whose effect id the run no longer awaits, or an end
//! of an activity that is not the run's live one, leaves the state as it is.

use std::collections::{BTreeMap, BTreeSet};

/// A point in time, in milliseconds since the Unix epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(pub u64);

/// The source of the time that an event carries (ORCH-1, 19.1). The engine
/// never calls it: the caller reads it when it builds the event.
pub trait Clock {
    /// The time now.
    fn now(&self) -> Timestamp;
}

/// A run id: a UUIDv7 (`GLOSSARY.md`), as its 128 bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RunId(pub u128);

/// The source of the run id of a claim (ORCH-1, 19.1). The engine never calls
/// it: the caller draws an id when it builds [`Event::Claim`].
pub trait IdGen {
    /// A run id that no run has had.
    fn run_id(&mut self) -> RunId;
}

/// A repository, by its GitHub id (18.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RepoId(pub u64);

/// An issue of a repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IssueKey {
    /// The repository.
    pub repo: RepoId,
    /// The issue number.
    pub number: u64,
}

/// A commit SHA.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sha(pub String);

/// The id of an effect: its run and the run's sequence number (8.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EffectId {
    /// The run that requested the effect.
    pub run: RunId,
    /// The run's sequence number of the request.
    pub seq: u64,
}

/// The id of an activity, a stage execution (ORCH-3). A restarted activity
/// gets a new one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ActivityId {
    /// The run of the stage execution.
    pub run: RunId,
    /// The run's sequence number of the start.
    pub seq: u64,
}

/// The id of a submitted pull request review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReviewId(pub u64);

/// A stage of the internal pipeline (7.3) or of a tier 2 parent (7.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stage {
    /// `claim` (8.2).
    Claim,
    /// `plan`.
    Plan,
    /// The plan gate (7.4). Not entered by the skeleton.
    PlanGate,
    /// `author_tests` (M3). Not entered by the skeleton.
    AuthorTests,
    /// `implement`.
    Implement,
    /// `capture` (9.4).
    Capture,
    /// `integrity` (11.3). Not entered by the skeleton.
    Integrity,
    /// `verify` (11.2).
    Verify,
    /// `publish`.
    Publish,
    /// `ci` (11.4).
    Ci,
    /// `review` (11.5).
    Review,
    /// `ready`.
    Ready,
    /// `human_review` (12.2).
    HumanReview,
    /// `merge` (12.3).
    Merge,
    /// `done`.
    Done,
    /// `decompose` (7.4). Not entered by the skeleton.
    Decompose,
    /// `await_children` (7.4). Not entered by the skeleton.
    AwaitChildren,
}

/// Where a run is: in the pipeline, or in one of the terminal states of a
/// run (`GLOSSARY.md`). A run in `needs-human` is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    /// Not terminal.
    Active,
    /// Terminal: the pull request was merged.
    Done,
    /// Terminal: the run was cancelled (7.7).
    Cancelled,
}

/// The category of an escalation (7.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscalationCategory {
    /// `clarification`.
    Clarification,
    /// `spec-conflict`.
    SpecConflict,
    /// `plan-approval`.
    PlanApproval,
    /// `budget`.
    Budget,
    /// `verification`.
    Verification,
    /// `review`.
    Review,
    /// `merge-conflict`.
    MergeConflict,
    /// `policy-denial`.
    PolicyDenial,
    /// `spec-changed`.
    SpecChanged,
    /// `infrastructure`.
    Infrastructure,
    /// `human-request`.
    HumanRequest,
}

/// The public state of 7.1, a `state:*` label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublicState {
    /// `state:triage`.
    Triage,
    /// `state:ready`.
    Ready,
    /// `state:in-progress`.
    InProgress,
    /// `state:in-review`.
    InReview,
    /// `state:needs-human`.
    NeedsHuman,
}

/// The round counters of a run (7.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rounds {
    /// Plan rounds (PLAN-3). The skeleton moves none.
    pub plan: u32,
    /// Verify rounds: capture, integrity and verify.
    pub verify: u32,
    /// CI rounds.
    pub ci: u32,
    /// Review rounds.
    pub review: u32,
    /// Human rounds.
    pub human: u32,
    /// Merge-conflict rounds (12.4). The skeleton moves none.
    pub merge_conflict: u32,
}

/// The round caps of 7.3 *(tunable)*. Exceeding a cap escalates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoundCaps {
    /// Plan rounds.
    pub plan: u32,
    /// Verify rounds.
    pub verify: u32,
    /// CI rounds.
    pub ci: u32,
    /// Review rounds.
    pub review: u32,
    /// Human rounds.
    pub human: u32,
    /// Merge-conflict rounds.
    pub merge_conflict: u32,
}

impl Default for RoundCaps {
    /// The caps of 7.3: plan 2, verify 3, CI 2, review 2, human 3,
    /// merge-conflict 2.
    fn default() -> Self {
        Self {
            plan: 2,
            verify: 3,
            ci: 2,
            review: 2,
            human: 3,
            merge_conflict: 2,
        }
    }
}

/// A gate whose failure returns the run to `implement` (7.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// Capture, integrity or verify.
    Verify,
    /// A red required check.
    Ci,
    /// The reviewer's blocking findings.
    Review,
    /// A human review that requests changes.
    Human,
}

impl Gate {
    fn counter(self, rounds: &mut Rounds) -> &mut u32 {
        match self {
            Self::Verify => &mut rounds.verify,
            Self::Ci => &mut rounds.ci,
            Self::Review => &mut rounds.review,
            Self::Human => &mut rounds.human,
        }
    }

    fn cap(self, caps: &RoundCaps) -> u32 {
        match self {
            Self::Verify => caps.verify,
            Self::Ci => caps.ci,
            Self::Review => caps.review,
            Self::Human => caps.human,
        }
    }

    fn category(self) -> EscalationCategory {
        match self {
            Self::Verify | Self::Ci => EscalationCategory::Verification,
            Self::Review | Self::Human => EscalationCategory::Review,
        }
    }
}

/// The merge strategy of a repository (12.3). `native_queue` is not
/// scheduled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeStrategy {
    /// The human merges; the factory finishes the run when it sees the merge.
    Human,
    /// The factory's merge coordinator merges.
    FactorySerial,
}

/// What the engine is given besides events: values of the policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineConfig {
    /// The round caps.
    pub caps: RoundCaps,
    /// The merge strategy.
    pub merge_strategy: MergeStrategy,
}

/// The kind of a GitHub account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorKind {
    /// An account of type User.
    User,
    /// An App or a bot.
    Bot,
}

/// The account behind a review, with the answer of the permission check that
/// the caller made through the GitHub API (SEC-1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    /// The login.
    pub login: String,
    /// The kind of the account.
    pub kind: ActorKind,
    /// Whether the account's permission was write, maintain or admin.
    pub write_permission: bool,
}

impl Actor {
    /// Whether the account is an authorised actor (`GLOSSARY.md`).
    #[must_use]
    pub fn is_authorised(&self) -> bool {
        self.kind == ActorKind::User && self.write_permission
    }
}

/// The verdict of a submitted review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewVerdict {
    /// Approved.
    Approved,
    /// Changes requested.
    ChangesRequested,
    /// Commented.
    Commented,
}

/// An approval that counts under PR-2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Approval {
    /// The review.
    pub review: ReviewId,
    /// The approver's login.
    pub by: String,
    /// The head SHA the review was submitted on.
    pub commit: Sha,
}

/// The conclusion of the required checks on a head (VAL-8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CiConclusion {
    /// Every required check satisfied.
    Green,
    /// A required check is red.
    Red,
}

/// How an external write ended. A transient failure never reaches the
/// engine: the executor retries it (8.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOutcome {
    /// The write took effect, or its key found it done (8.5).
    Succeeded,
    /// A Deterministic failure (8.6).
    Failed,
}

/// How a stage execution ended (ORCH-4). What each means depends on the
/// stage; an outcome that the stage cannot have leaves the state as it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivityOutcome {
    /// `plan` returned a plan, `implement` a result, `verify` passed, or
    /// `review` returned no blocking finding.
    Succeeded,
    /// `capture` built the candidate with this SHA.
    Captured(Sha),
    /// `capture` rejected a path, `verify` failed, or `review` returned a
    /// blocking finding.
    GateFailed,
    /// An agent stopped with status `needs_human` (7.5).
    NeedsHuman,
    /// An agent failure (8.6), or a capture or verify that could not run.
    Failed,
}

/// Why a run is cancelled (7.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelCause {
    /// The issue was closed and the pull request is not merged.
    IssueClosed,
    /// A human applied `state:triage`.
    TriageApplied,
}

/// An event that the engine steps on (ORCH-1, ORCH-4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Claim an issue whose approval the caller checked (INV-4, 8.2). The run
    /// id comes from [`IdGen`] and the time from [`Clock`].
    Claim {
        /// The new run's id.
        run: RunId,
        /// The issue.
        issue: IssueKey,
        /// The default-branch head, both `base_sha` and `policy_sha`.
        base_sha: Sha,
        /// The time of the claim.
        at: Timestamp,
    },
    /// The result of an external write, with its effect id (ORCH-3).
    WriteCompleted {
        /// The effect.
        effect: EffectId,
        /// How it ended.
        outcome: WriteOutcome,
    },
    /// A stage execution started. It is recorded and changes no state.
    ActivityStarted {
        /// The activity.
        activity: ActivityId,
    },
    /// A stage execution ended.
    ActivityEnded {
        /// The activity.
        activity: ActivityId,
        /// How it ended.
        outcome: ActivityOutcome,
    },
    /// A stage execution was killed after a restart and marked abandoned
    /// (ORCH-3).
    ActivityAbandoned {
        /// The activity.
        activity: ActivityId,
    },
    /// The required checks on a head concluded.
    ChecksConcluded {
        /// The run.
        run: RunId,
        /// The head SHA the checks ran on.
        head: Sha,
        /// The conclusion.
        conclusion: CiConclusion,
    },
    /// A review was submitted on the run's pull request (PR-1).
    ReviewSubmitted {
        /// The run.
        run: RunId,
        /// The review.
        review: ReviewId,
        /// Who submitted it.
        reviewer: Actor,
        /// The head SHA it was submitted on.
        commit: Sha,
        /// The verdict.
        verdict: ReviewVerdict,
    },
    /// The run's pull request was merged (7.7).
    PullRequestMerged {
        /// The run.
        run: RunId,
    },
    /// A cancel row of 7.7 was observed.
    CancelObserved {
        /// The run.
        run: RunId,
        /// The row.
        cause: CancelCause,
    },
}

/// An external write (ORCH-3). The executor finds the repository, the issue,
/// the branch and the pull request on the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Write {
    /// Create `refs/heads/factory/issue-<n>` at `base_sha` (8.2).
    CreateBranch {
        /// The commit of the ref.
        base_sha: Sha,
    },
    /// Set this `state:*` label and remove the others.
    SetState(PublicState),
    /// Create the status comment (5.4).
    CreateStatusComment,
    /// Update the status comment; the final one when the run ends.
    UpdateStatusComment,
    /// Post the comment of an escalation (7.5).
    PostEscalation(EscalationCategory),
    /// Push the candidate to the work branch.
    Push {
        /// The candidate.
        candidate: Sha,
    },
    /// Create the draft pull request, or update it.
    UpsertPullRequest,
    /// Mark the pull request ready and request a review (4.2).
    MarkReady,
    /// Merge the pull request at this head (12.3).
    Merge {
        /// The head SHA the approval was given on.
        head: Sha,
    },
    /// Close the pull request.
    ClosePullRequest,
}

/// An effect that [`step`] returns (ORCH-3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// An external write.
    Write {
        /// Its id; the result event carries it.
        id: EffectId,
        /// The write.
        write: Write,
    },
    /// Start a stage execution of the stage.
    StartActivity {
        /// Its id; the activity events carry it.
        id: ActivityId,
        /// The stage.
        stage: Stage,
    },
    /// Interrupt a live stage execution through the stop sequence (AG-12).
    InterruptActivity {
        /// The activity.
        id: ActivityId,
    },
}

/// The state of one run: what its `runs` row holds (ORCH-2, ORCH-6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    /// The run id.
    pub id: RunId,
    /// The issue.
    pub issue: IssueKey,
    /// When it was claimed.
    pub claimed_at: Timestamp,
    /// `base_sha` and `policy_sha` (8.2).
    pub base_sha: Sha,
    /// The attempt (7.6).
    pub attempt: u32,
    /// The recorded stage.
    pub stage: Stage,
    /// Active or terminal.
    pub status: RunStatus,
    /// The category, while the run is in `needs-human`.
    pub escalation: Option<EscalationCategory>,
    /// The round counters.
    pub rounds: Rounds,
    /// Restarts of the current stage after an agent failure (8.6).
    pub restarts: u32,
    /// Abandoned activities of the current stage, toward Poison (8.6).
    pub abandoned: u32,
    /// The live stage execution (INV-6).
    pub activity: Option<ActivityId>,
    /// The writes of the current stage that have no result yet.
    pub awaiting: BTreeSet<EffectId>,
    /// The candidate of the current round (ORCH-6).
    pub candidate: Option<Sha>,
    /// The head that the factory last pushed.
    pub head: Option<Sha>,
    /// Every SHA that the factory pushed.
    pub pushed: BTreeSet<Sha>,
    /// Whether the pull request exists.
    pub pull_request: bool,
    /// The approval that counts under PR-2, on `head`.
    pub approval: Option<Approval>,
    /// The reviews that were applied; one starts at most one round.
    pub reviews: BTreeSet<ReviewId>,
    /// The next sequence number of the run.
    pub next_seq: u64,
}

impl Run {
    /// Whether the run is in a terminal state.
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        self.status != RunStatus::Active
    }
}

/// All run state, which one engine task owns (ORCH-2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    /// The values of the policy.
    pub config: EngineConfig,
    /// Every run, by its id.
    pub runs: BTreeMap<RunId, Run>,
}

impl State {
    /// A state with no run.
    #[must_use]
    pub fn new(config: EngineConfig) -> Self {
        Self {
            config,
            runs: BTreeMap::new(),
        }
    }

    /// The non-terminal run of an issue, if there is one (INV-6).
    #[must_use]
    pub fn active_run(&self, issue: IssueKey) -> Option<&Run> {
        self.runs
            .values()
            .find(|run| run.issue == issue && !run.is_terminal())
    }
}

/// Steps the engine on one event (ORCH-1).
#[must_use]
pub fn step(mut state: State, event: Event) -> (State, Vec<Effect>) {
    let mut effects = Vec::new();
    let config = state.config;
    match event {
        Event::Claim {
            run,
            issue,
            base_sha,
            at,
        } => {
            if !state.runs.contains_key(&run) && state.active_run(issue).is_none() {
                let run = claim(run, issue, base_sha, at, config, &mut effects);
                state.runs.insert(run.id, run);
            }
        }
        Event::WriteCompleted { effect, outcome } => {
            if let Some(run) = state.runs.get_mut(&effect.run) {
                Step::new(run, config, &mut effects).write_completed(effect, outcome);
            }
        }
        Event::ActivityStarted { .. } => {}
        Event::ActivityEnded { activity, outcome } => {
            if let Some(run) = state.runs.get_mut(&activity.run) {
                Step::new(run, config, &mut effects).activity_ended(activity, outcome);
            }
        }
        Event::ActivityAbandoned { activity } => {
            if let Some(run) = state.runs.get_mut(&activity.run) {
                Step::new(run, config, &mut effects).activity_abandoned(activity);
            }
        }
        Event::ChecksConcluded {
            run,
            head,
            conclusion,
        } => {
            if let Some(run) = state.runs.get_mut(&run) {
                Step::new(run, config, &mut effects).checks_concluded(&head, conclusion);
            }
        }
        Event::ReviewSubmitted {
            run,
            review,
            reviewer,
            commit,
            verdict,
        } => {
            if let Some(run) = state.runs.get_mut(&run) {
                Step::new(run, config, &mut effects)
                    .review_submitted(review, &reviewer, commit, verdict);
            }
        }
        Event::PullRequestMerged { run } => {
            if let Some(run) = state.runs.get_mut(&run) {
                Step::new(run, config, &mut effects).pull_request_merged();
            }
        }
        Event::CancelObserved { run, .. } => {
            if let Some(run) = state.runs.get_mut(&run) {
                Step::new(run, config, &mut effects).cancel();
            }
        }
    }
    (state, effects)
}

/// The run of a claim, with the writes of `claim` (8.2) awaited.
fn claim(
    id: RunId,
    issue: IssueKey,
    base_sha: Sha,
    at: Timestamp,
    config: EngineConfig,
    effects: &mut Vec<Effect>,
) -> Run {
    let mut run = Run {
        id,
        issue,
        claimed_at: at,
        base_sha: base_sha.clone(),
        attempt: 1,
        stage: Stage::Claim,
        status: RunStatus::Active,
        escalation: None,
        rounds: Rounds::default(),
        restarts: 0,
        abandoned: 0,
        activity: None,
        awaiting: BTreeSet::new(),
        candidate: None,
        head: None,
        pushed: BTreeSet::new(),
        pull_request: false,
        approval: None,
        reviews: BTreeSet::new(),
        next_seq: 0,
    };
    let mut step = Step::new(&mut run, config, effects);
    step.await_write(Write::CreateBranch { base_sha });
    step.await_write(Write::SetState(PublicState::InProgress));
    step.await_write(Write::CreateStatusComment);
    run
}

/// One step of one run.
struct Step<'a> {
    run: &'a mut Run,
    config: EngineConfig,
    effects: &'a mut Vec<Effect>,
}

impl<'a> Step<'a> {
    fn new(run: &'a mut Run, config: EngineConfig, effects: &'a mut Vec<Effect>) -> Self {
        Self {
            run,
            config,
            effects,
        }
    }

    /// Whether the run takes the events of its pipeline: it is active and
    /// not in `needs-human` (7.5).
    fn in_pipeline(&self) -> bool {
        !self.run.is_terminal() && self.run.escalation.is_none()
    }

    fn next_seq(&mut self) -> u64 {
        let seq = self.run.next_seq;
        self.run.next_seq += 1;
        seq
    }

    /// Requests a write whose result the stage does not wait for.
    fn write(&mut self, write: Write) -> EffectId {
        let id = EffectId {
            run: self.run.id,
            seq: self.next_seq(),
        };
        self.effects.push(Effect::Write { id, write });
        id
    }

    /// Requests a write that the stage waits for.
    fn await_write(&mut self, write: Write) {
        let id = self.write(write);
        self.run.awaiting.insert(id);
    }

    fn start_activity(&mut self) {
        let id = ActivityId {
            run: self.run.id,
            seq: self.next_seq(),
        };
        self.run.activity = Some(id);
        self.effects.push(Effect::StartActivity {
            id,
            stage: self.run.stage,
        });
    }

    fn interrupt_activity(&mut self) {
        if let Some(id) = self.run.activity.take() {
            self.effects.push(Effect::InterruptActivity { id });
        }
    }

    /// Enters a stage at its start.
    fn enter(&mut self, stage: Stage) {
        self.run.stage = stage;
        self.run.restarts = 0;
        self.run.abandoned = 0;
        match stage {
            Stage::Plan | Stage::Implement | Stage::Capture | Stage::Verify | Stage::Review => {
                self.start_activity();
            }
            Stage::Publish => {
                if let Some(candidate) = self.run.candidate.clone() {
                    self.await_write(Write::Push { candidate });
                }
                self.await_write(Write::UpsertPullRequest);
            }
            Stage::Ready => {
                self.await_write(Write::MarkReady);
                self.await_write(Write::SetState(PublicState::InReview));
            }
            Stage::Merge => {
                if let Some(head) = self.run.head.clone() {
                    self.await_write(Write::Merge { head });
                }
            }
            Stage::Done => {
                self.run.status = RunStatus::Done;
                self.write(Write::UpdateStatusComment);
            }
            _ => {}
        }
    }

    /// Leaves a stage whose writes all have their results.
    fn stage_written(&mut self) {
        match self.run.stage {
            Stage::Claim => self.enter(Stage::Plan),
            Stage::Publish => {
                if let Some(candidate) = self.run.candidate.clone() {
                    self.run.pushed.insert(candidate.clone());
                    self.run.head = Some(candidate);
                    // PR-2: a push invalidates every earlier approval.
                    self.run.approval = None;
                }
                self.run.pull_request = true;
                self.enter(Stage::Ci);
            }
            Stage::Ready => self.enter(Stage::HumanReview),
            Stage::Merge => self.enter(Stage::Done),
            _ => {}
        }
    }

    /// Escalates (7.5): the run stays active, in its recorded stage.
    ///
    /// The skeleton escalates only where no activity is live: at the end of
    /// one, or at a result that a wait stage or a writing stage receives.
    fn escalate(&mut self, category: EscalationCategory) {
        self.run.awaiting.clear();
        self.run.escalation = Some(category);
        self.write(Write::SetState(PublicState::NeedsHuman));
        self.write(Write::PostEscalation(category));
    }

    /// A failed gate: a round, or an escalation where the round would exceed
    /// the cap (7.3).
    fn fail_gate(&mut self, gate: Gate) {
        let cap = gate.cap(&self.config.caps);
        let counter = gate.counter(&mut self.run.rounds);
        if *counter >= cap {
            self.escalate(gate.category());
            return;
        }
        *counter += 1;
        if gate == Gate::Human {
            self.write(Write::SetState(PublicState::InProgress));
        }
        self.run.candidate = None;
        self.enter(Stage::Implement);
    }

    fn write_completed(&mut self, effect: EffectId, outcome: WriteOutcome) {
        if !self.in_pipeline() || !self.run.awaiting.remove(&effect) {
            return;
        }
        match outcome {
            WriteOutcome::Failed => self.escalate(EscalationCategory::Infrastructure),
            WriteOutcome::Succeeded => {
                if self.run.awaiting.is_empty() {
                    self.stage_written();
                }
            }
        }
    }

    fn activity_ended(&mut self, activity: ActivityId, outcome: ActivityOutcome) {
        if !self.in_pipeline() || self.run.activity != Some(activity) {
            return;
        }
        let stage = self.run.stage;
        let agent = matches!(stage, Stage::Plan | Stage::Implement | Stage::Review);
        match (stage, outcome) {
            (Stage::Plan, ActivityOutcome::Succeeded) => self.end_and_enter(Stage::Implement),
            (Stage::Implement, ActivityOutcome::Succeeded) => self.end_and_enter(Stage::Capture),
            (Stage::Capture, ActivityOutcome::Captured(candidate)) => {
                self.run.candidate = Some(candidate);
                self.end_and_enter(Stage::Verify);
            }
            (Stage::Verify, ActivityOutcome::Succeeded) => self.end_and_enter(Stage::Publish),
            (Stage::Review, ActivityOutcome::Succeeded) => self.end_and_enter(Stage::Ready),
            (Stage::Capture | Stage::Verify, ActivityOutcome::GateFailed) => {
                self.run.activity = None;
                self.fail_gate(Gate::Verify);
            }
            (Stage::Review, ActivityOutcome::GateFailed) => {
                self.run.activity = None;
                self.fail_gate(Gate::Review);
            }
            (_, ActivityOutcome::NeedsHuman) if agent => {
                self.run.activity = None;
                self.escalate(EscalationCategory::Clarification);
            }
            (_, ActivityOutcome::Failed) if agent => {
                self.run.activity = None;
                if self.run.restarts == 0 {
                    self.run.restarts = 1;
                    self.start_activity();
                } else {
                    self.escalate(EscalationCategory::Verification);
                }
            }
            (Stage::Capture | Stage::Verify, ActivityOutcome::Failed) => {
                self.run.activity = None;
                self.escalate(EscalationCategory::Infrastructure);
            }
            _ => {}
        }
    }

    fn end_and_enter(&mut self, stage: Stage) {
        self.run.activity = None;
        self.enter(stage);
    }

    /// Poison (8.6): the third abandoned activity of a stage escalates.
    fn activity_abandoned(&mut self, activity: ActivityId) {
        if !self.in_pipeline() || self.run.activity != Some(activity) {
            return;
        }
        self.run.activity = None;
        self.run.abandoned += 1;
        if self.run.abandoned >= 3 {
            self.escalate(EscalationCategory::Infrastructure);
        } else {
            self.start_activity();
        }
    }

    fn checks_concluded(&mut self, head: &Sha, conclusion: CiConclusion) {
        if !self.in_pipeline()
            || self.run.stage != Stage::Ci
            || self.run.head.as_ref() != Some(head)
        {
            return;
        }
        match conclusion {
            CiConclusion::Green => self.enter(Stage::Review),
            CiConclusion::Red => self.fail_gate(Gate::Ci),
        }
    }

    fn review_submitted(
        &mut self,
        review: ReviewId,
        reviewer: &Actor,
        commit: Sha,
        verdict: ReviewVerdict,
    ) {
        if !self.in_pipeline()
            || self.run.stage != Stage::HumanReview
            || !self.run.reviews.insert(review)
            || !reviewer.is_authorised()
        {
            return;
        }
        match verdict {
            ReviewVerdict::ChangesRequested => self.fail_gate(Gate::Human),
            ReviewVerdict::Approved if self.run.head.as_ref() == Some(&commit) => {
                self.run.approval = Some(Approval {
                    review,
                    by: reviewer.login.clone(),
                    commit,
                });
                if self.config.merge_strategy == MergeStrategy::FactorySerial {
                    self.enter(Stage::Merge);
                }
            }
            ReviewVerdict::Approved | ReviewVerdict::Commented => {}
        }
    }

    /// The first row of 7.7, which 7.5 also evaluates in `needs-human`.
    fn pull_request_merged(&mut self) {
        if self.run.is_terminal() || !self.run.pull_request {
            return;
        }
        self.interrupt_activity();
        self.run.awaiting.clear();
        self.run.escalation = None;
        self.enter(Stage::Done);
    }

    /// The cancel rows of 7.7, which 7.5 also evaluates in `needs-human`.
    fn cancel(&mut self) {
        if self.run.is_terminal() {
            return;
        }
        self.interrupt_activity();
        self.run.awaiting.clear();
        self.run.status = RunStatus::Cancelled;
        if self.run.pull_request {
            self.write(Write::ClosePullRequest);
        }
    }
}
