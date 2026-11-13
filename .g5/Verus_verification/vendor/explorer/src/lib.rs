//! An explicit-state explorer for our own state machines.
//!
//! A [`Model`] supplies initial states, the actions enabled in a state (drawn from finite,
//! declared parameter domains) and a deterministic next-state function. In this repository the
//! next-state functions are the executable twins that Verus's `exec_spec_verified!` compiles
//! from the spec transitions, so the machine explored here is proved equal to the machine
//! proved in Verus.
//!
//! The explorer searches breadth first, so the first counterexample it finds is a shortest
//! trace. It checks:
//! - state invariants, in every reachable state;
//! - action properties, on every explored step (pre-state, action, post-state);
//! - "always possibly" properties (AG EF goal): from every reachable state some path reaches a
//!   goal state, the bounded analogue of "it can always still settle";
//! - deadlocks, if requested: reachable states with no enabled action.
//!
//! Finding nothing is evidence, not proof: the guarantee for all sizes comes from the Verus
//! proof of the same invariant. A bug in this explorer can hide a counterexample but cannot
//! make a false property verify.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, VecDeque};
use std::hash::Hasher;
use std::fmt::Debug;
use std::hash::Hash;
use std::time::Instant;

/// A state machine to explore.
pub trait Model {
    type State: Clone + Eq + Hash + Debug;
    type Action: Clone + Debug;

    fn init_states(&self) -> Vec<Self::State>;
    /// Every action whose parameters come from the model's declared finite domains. An action
    /// may be returned even if it is not enabled; [`Model::next`] decides.
    fn actions(&self, s: &Self::State) -> Vec<Self::Action>;
    /// `None` when the action is not enabled in `s`.
    fn next(&self, s: &Self::State, a: &Self::Action) -> Option<Self::State>;

    fn invariants(&self) -> Vec<Invariant<Self::State>> {
        vec![]
    }
    fn action_properties(&self) -> Vec<ActionProperty<Self::State, Self::Action>> {
        vec![]
    }
    fn always_possibly(&self) -> Vec<Invariant<Self::State>> {
        vec![]
    }
    /// States beyond which the search does not expand (bounds such as "at most N requests").
    fn within_bounds(&self, _s: &Self::State) -> bool {
        true
    }
}

pub struct Invariant<S> {
    pub name: &'static str,
    pub holds: Box<dyn Fn(&S) -> bool>,
}

pub struct ActionProperty<S, A> {
    pub name: &'static str,
    pub holds: Box<dyn Fn(&S, &A, &S) -> bool>,
}

pub fn invariant<S>(name: &'static str, f: impl Fn(&S) -> bool + 'static) -> Invariant<S> {
    Invariant { name, holds: Box::new(f) }
}

pub fn action_property<S, A>(name: &'static str, f: impl Fn(&S, &A, &S) -> bool + 'static) -> ActionProperty<S, A> {
    ActionProperty { name, holds: Box::new(f) }
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_states: usize,
    pub max_depth: usize,
    pub check_deadlock: bool,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { max_states: 5_000_000, max_depth: usize::MAX, check_deadlock: false }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViolationKind {
    Invariant,
    ActionProperty,
    AlwaysPossibly,
    Deadlock,
}

/// A shortest trace from an initial state to the violation. Each step is the action taken and
/// the state it produced; `init` is the starting state.
#[derive(Debug, Clone)]
pub struct Counterexample<S, A> {
    pub property: String,
    pub kind: ViolationKind,
    pub init: S,
    pub steps: Vec<(A, S)>,
}

#[derive(Debug)]
pub struct Report<S, A> {
    pub states: usize,
    pub transitions: usize,
    pub depth: usize,
    /// True when the search stopped at `max_states` or `max_depth` before exhausting the space.
    pub truncated: bool,
    pub seconds: f64,
    pub violation: Option<Counterexample<S, A>>,
}

impl<S: Debug, A: Debug> Report<S, A> {
    pub fn passed(&self) -> bool {
        self.violation.is_none()
    }

    pub fn summary(&self) -> String {
        let scope = if self.truncated { "bounded (truncated)" } else { "exhaustive" };
        match &self.violation {
            None => format!(
                "PASS: {} states, {} transitions, depth {}, {scope}, {:.2}s",
                self.states, self.transitions, self.depth, self.seconds
            ),
            Some(c) => format!(
                "VIOLATION of {} ({:?}) after {} steps; {} states explored, {:.2}s",
                c.property, c.kind, c.steps.len(), self.states, self.seconds
            ),
        }
    }

    pub fn trace(&self) -> String {
        let Some(c) = &self.violation else { return String::new() };
        let mut out = format!("Counterexample for {}:\n  init: {:?}\n", c.property, c.init);
        for (i, (a, s)) in c.steps.iter().enumerate() {
            out.push_str(&format!("  {:>3}. {:?}\n       -> {:?}\n", i + 1, a, s));
        }
        out
    }
}

struct Node<S, A> {
    state: S,
    parent: Option<(usize, A)>,
    depth: usize,
}

pub fn explore<M: Model>(model: &M, limits: Limits) -> Report<M::State, M::Action> {
    let start = Instant::now();
    let invariants = model.invariants();
    let action_props = model.action_properties();
    let goals = model.always_possibly();

    let mut nodes: Vec<Node<M::State, M::Action>> = Vec::new();
    // States are stored once, in `nodes`; the index maps a state's hash to the nodes with it.
    let mut index: HashMap<u64, Vec<usize>> = HashMap::new();
    let hash_of = |s: &M::State| { let mut h = DefaultHasher::new(); s.hash(&mut h); h.finish() };
    let mut queue: VecDeque<usize> = VecDeque::new();
    // Successor lists, kept only when an AG EF check needs the graph.
    let mut edges: Vec<Vec<usize>> = Vec::new();
    let keep_edges = !goals.is_empty();
    // States the search did not expand (at a bound or at the depth limit).
    let mut unexpanded: Vec<usize> = Vec::new();
    let mut transitions = 0usize;
    let mut depth = 0usize;
    let mut truncated = false;

    let trace_to = |nodes: &Vec<Node<M::State, M::Action>>, mut i: usize| {
        let mut steps = Vec::new();
        while let Some((p, a)) = &nodes[i].parent {
            steps.push((a.clone(), nodes[i].state.clone()));
            i = *p;
        }
        steps.reverse();
        (nodes[i].state.clone(), steps)
    };

    macro_rules! fail {
        ($name:expr, $kind:expr, $i:expr) => {{
            let (init, steps) = trace_to(&nodes, $i);
            return Report {
                states: nodes.len(),
                transitions,
                depth,
                truncated,
                seconds: start.elapsed().as_secs_f64(),
                violation: Some(Counterexample { property: $name.to_string(), kind: $kind, init, steps }),
            };
        }};
    }

    for s in model.init_states() {
        let h = hash_of(&s);
        if index.get(&h).is_some_and(|v| v.iter().any(|&j| nodes[j].state == s)) {
            continue;
        }
        let i = nodes.len();
        index.entry(h).or_default().push(i);
        nodes.push(Node { state: s, parent: None, depth: 0 });
        if keep_edges {
            edges.push(Vec::new());
        }
        for inv in &invariants {
            if !(inv.holds)(&nodes[i].state) {
                fail!(inv.name, ViolationKind::Invariant, i);
            }
        }
        queue.push_back(i);
    }

    while let Some(i) = queue.pop_front() {
        let d = nodes[i].depth;
        depth = depth.max(d);
        if d >= limits.max_depth || !model.within_bounds(&nodes[i].state) {
            if d >= limits.max_depth {
                truncated = true;
            }
            unexpanded.push(i);
            continue;
        }
        let state = nodes[i].state.clone();
        let mut enabled = 0usize;
        for a in model.actions(&state) {
            let Some(next) = model.next(&state, &a) else { continue };
            enabled += 1;
            transitions += 1;
            for p in &action_props {
                if !(p.holds)(&state, &a, &next) {
                    // Record the offending step so the trace ends with it.
                    let j = nodes.len();
                    nodes.push(Node { state: next.clone(), parent: Some((i, a.clone())), depth: d + 1 });
                    fail!(p.name, ViolationKind::ActionProperty, j);
                }
            }
            let h = hash_of(&next);
            let found = index.get(&h).and_then(|v| v.iter().copied().find(|&j| nodes[j].state == next));
            let j = match found {
                Some(j) => j,
                None => {
                    if nodes.len() >= limits.max_states {
                        truncated = true;
                        continue;
                    }
                    let j = nodes.len();
                    index.entry(h).or_default().push(j);
                    nodes.push(Node { state: next, parent: Some((i, a.clone())), depth: d + 1 });
                    if keep_edges {
                        edges.push(Vec::new());
                    }
                    for inv in &invariants {
                        if !(inv.holds)(&nodes[j].state) {
                            fail!(inv.name, ViolationKind::Invariant, j);
                        }
                    }
                    queue.push_back(j);
                    j
                }
            };
            if keep_edges {
                edges[i].push(j);
            }
        }
        if limits.check_deadlock && enabled == 0 {
            fail!("deadlock", ViolationKind::Deadlock, i);
        }
    }

    // AG EF goal: every reachable state can reach a goal state. Computed backwards from the
    // goal states over the explored graph. States at the frontier (not expanded because of a
    // bound, the depth limit or the state limit) are treated as able to reach the goal, so a
    // bounded run can miss violations but never reports a false one.
    for g in &goals {
        let n = nodes.len();
        let mut preds: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (u, succ) in edges.iter().enumerate() {
            for &v in succ {
                preds[v].push(u);
            }
        }
        let mut can = vec![false; n];
        let mut frontier = vec![false; n];
        for &u in &unexpanded {
            frontier[u] = true;
        }
        let mut work: Vec<usize> = Vec::new();
        for u in 0..n {
            let at_state_limit = truncated && edges[u].is_empty() && !frontier[u];
            if (g.holds)(&nodes[u].state) || frontier[u] || at_state_limit {
                can[u] = true;
                work.push(u);
            }
        }
        while let Some(v) = work.pop() {
            for &u in &preds[v] {
                if !can[u] {
                    can[u] = true;
                    work.push(u);
                }
            }
        }
        if let Some(bad) = (0..n).find(|&u| !can[u]) {
            fail!(g.name, ViolationKind::AlwaysPossibly, bad);
        }
    }

    Report {
        states: nodes.len(),
        transitions,
        depth,
        truncated,
        seconds: start.elapsed().as_secs_f64(),
        violation: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two clients increment a shared counter with a non-atomic read then write: the classic
    /// lost update. The explorer must find it, with a shortest trace.
    #[derive(Clone, PartialEq, Eq, Hash, Debug)]
    struct S {
        counter: u8,
        pc: [u8; 2],
        local: [u8; 2],
    }
    #[derive(Clone, Debug)]
    enum A {
        Read(usize),
        Write(usize),
    }
    struct LostUpdate;
    impl Model for LostUpdate {
        type State = S;
        type Action = A;
        fn init_states(&self) -> Vec<S> {
            vec![S { counter: 0, pc: [0, 0], local: [0, 0] }]
        }
        fn actions(&self, _s: &S) -> Vec<A> {
            vec![A::Read(0), A::Read(1), A::Write(0), A::Write(1)]
        }
        fn next(&self, s: &S, a: &A) -> Option<S> {
            let mut t = s.clone();
            match *a {
                A::Read(c) if s.pc[c] == 0 => {
                    t.local[c] = s.counter;
                    t.pc[c] = 1;
                }
                A::Write(c) if s.pc[c] == 1 => {
                    t.counter = s.local[c] + 1;
                    t.pc[c] = 2;
                }
                _ => return None,
            }
            Some(t)
        }
        fn invariants(&self) -> Vec<Invariant<S>> {
            vec![invariant("both increments count", |s: &S| s.pc != [2, 2] || s.counter == 2)]
        }
    }

    #[test]
    fn finds_lost_update_with_shortest_trace() {
        let r = explore(&LostUpdate, Limits::default());
        let c = r.violation.expect("lost update must be found");
        assert_eq!(c.property, "both increments count");
        assert_eq!(c.steps.len(), 4);
    }

    /// A ticket can be taken and released; with "release" removed it can never be free again.
    #[derive(Clone, PartialEq, Eq, Hash, Debug)]
    struct T(bool);
    #[derive(Clone, Debug)]
    enum TA {
        Take,
        Release,
    }
    struct Ticket {
        release: bool,
    }
    impl Model for Ticket {
        type State = T;
        type Action = TA;
        fn init_states(&self) -> Vec<T> {
            vec![T(false)]
        }
        fn actions(&self, _s: &T) -> Vec<TA> {
            vec![TA::Take, TA::Release]
        }
        fn next(&self, s: &T, a: &TA) -> Option<T> {
            match a {
                TA::Take if !s.0 => Some(T(true)),
                TA::Release if s.0 && self.release => Some(T(false)),
                _ => None,
            }
        }
        fn always_possibly(&self) -> Vec<Invariant<T>> {
            vec![invariant("can be free again", |s: &T| !s.0)]
        }
    }

    #[test]
    fn always_possibly_holds_and_fails() {
        assert!(explore(&Ticket { release: true }, Limits::default()).passed());
        let r = explore(&Ticket { release: false }, Limits::default());
        assert_eq!(r.violation.unwrap().kind, ViolationKind::AlwaysPossibly);
    }

    #[test]
    fn action_property_reports_the_offending_step() {
        struct NoWrite;
        impl Model for NoWrite {
            type State = S;
            type Action = A;
            fn init_states(&self) -> Vec<S> {
                LostUpdate.init_states()
            }
            fn actions(&self, s: &S) -> Vec<A> {
                LostUpdate.actions(s)
            }
            fn next(&self, s: &S, a: &A) -> Option<S> {
                LostUpdate.next(s, a)
            }
            fn action_properties(&self) -> Vec<ActionProperty<S, A>> {
                vec![action_property("counter never moves", |a: &S, _: &A, b: &S| a.counter == b.counter)]
            }
        }
        let c = explore(&NoWrite, Limits::default()).violation.unwrap();
        assert_eq!(c.kind, ViolationKind::ActionProperty);
        assert!(matches!(c.steps.last().unwrap().0, A::Write(_)));
    }
}

/// Implement `Clone` and `Debug` for an exec struct generated by `exec_spec_verified!` (which
/// derives `PartialEq`, `Eq` and `Hash` but not these), field by field.
#[macro_export]
macro_rules! state_struct {
    ($t:ident { $($f:ident),* $(,)? }) => {
        impl Clone for $t { fn clone(&self) -> Self { $t { $($f: self.$f.clone()),* } } }
        impl std::fmt::Debug for $t {
            fn fmt(&self, fm: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                fm.debug_struct(stringify!($t)) $(.field(stringify!($f), &self.$f))* .finish()
            }
        }
    };
}
