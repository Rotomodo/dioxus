//! Suspense boundaries nested the way an app's board screen nests them: a shell boundary, a
//! layout boundary around a keyed frame with two roots, and a boundary inside the frame's `main`,
//! with pages that wait (suspend) only the first time they load something.
//!
//! The fuzz test drives random navigation, unrelated re-renders and loads finishing, and checks
//! the renderer against the VirtualDom after every render. `SEEDS` and `FIRST_SEED` widen or
//! replay it. The other tests are cases it found:
//!
//! - a boundary that suspended, resolved, and suspended again put its fallback (and then its
//!   children) at the renderer root, its new fallback mount having no render parent;
//! - a fallback's own components lost their edits when the boundary re-diffed;
//! - a boundary hidden in another boundary's retained branch wrote its new fallback to the
//!   renderer.

use std::{cell::RefCell, collections::HashSet, time::Duration};

use dioxus::prelude::*;
use dioxus_core::{ScopeId, Task};
use dioxus_renderer_oracle::RendererOracle;

/// Where the app is: a board, and what's open in it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Place {
    Feed,
    Overview,
    Space(u8),
    Item(u8),
    Page(u8),
}

static ROUTE: GlobalSignal<(u8, Place)> = Signal::global(|| (0, Place::Overview));
static SHELL_TICK: GlobalSignal<u32> = Signal::global(|| 0);
static FRAME_TICK: GlobalSignal<u32> = Signal::global(|| 0);
static SIDE_TICK: GlobalSignal<u32> = Signal::global(|| 0);
/// The loaders' insects' step: they re-render when it changes.
static INSECT: GlobalSignal<u32> = Signal::global(|| 0);

thread_local! {
    /// What's been loaded, kept across pages (not reactive, as the app's cache isn't read
    /// reactively either).
    static CACHE: RefCell<HashSet<u32>> = RefCell::new(HashSet::new());
    /// Loads still out: each finishes when its sender is sent on (or dropped).
    static PENDING: RefCell<Vec<tokio::sync::oneshot::Sender<()>>> = const { RefCell::new(Vec::new()) };
}

/// The app's `use_cached`: waits (suspends) only on its first render, when nothing's kept;
/// after that, a key with nothing kept gives `false` (the loading arm) until it's in.
fn use_cached_model(key: u32) -> Result<bool, RenderError> {
    let mut shown = use_hook(|| CopyValue::new(false));
    let done = use_signal(HashSet::<u32>::new);
    let pending = use_hook(|| CopyValue::new(Vec::<(u32, Task)>::new()));
    if CACHE.with(|cache| cache.borrow().contains(&key)) || done.read().contains(&key) {
        shown.set(true);
        return Ok(true);
    }
    let found = pending
        .read()
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, task)| *task);
    let task = match found {
        Some(task) => task,
        None => {
            let (release, released) = tokio::sync::oneshot::channel();
            PENDING.with(|pending| pending.borrow_mut().push(release));
            let mut done = done;
            let task = spawn(async move {
                let _ = released.await;
                CACHE.with(|cache| cache.borrow_mut().insert(key));
                done.write().insert(key);
            });
            let mut pending = pending;
            pending.write().push((key, task));
            task
        }
    };
    if !*shown.peek() {
        suspend(task)?;
    }
    shown.set(true);
    Ok(false)
}

fn app() -> Element {
    rsx! {
        div { class: "app",
            style { "css" }
            Shell {}
        }
    }
}

#[component]
fn Shell() -> Element {
    let _ = ROUTE();
    let tick = SHELL_TICK();
    let signed_in = true;
    let me = "me";
    rsx! {
        Empty {}
        if signed_in {
            nav { class: "rail", "data-tick": "{tick}", "rail" }
        }
        SuspenseBoundary {
            fallback: |_| rsx! { Loading {} },
            ShellOutlet {}
        }
        Empty {}
        div { "popover": "auto", "verify" }
        if signed_in {
            Empty { key: "{me}" }
        }
        if signed_in {
            Empty { key: "{me}" }
        }
        if signed_in {
            Tabs { key: "{me}" }
        }
        if signed_in {
            Empty { key: "{me}" }
        }
    }
}

#[component]
fn Tabs() -> Element {
    let route = ROUTE();
    rsx! { div { class: "tabs", "{route:?}" } }
}

/// Renders no DOM, like `document::Title` and the shell's quiet parts.
#[component]
fn Empty() -> Element {
    VNode::empty()
}

#[component]
fn ShellOutlet() -> Element {
    let (board, place) = ROUTE();
    match place {
        Place::Feed => rsx! {
            TopBar {}
            main { class: "feed", "feed" }
        },
        _ => rsx! { BoardLayout { id: board } },
    }
}

#[component]
fn TopBar() -> Element {
    rsx! { header { class: "topbar", "top" } }
}

#[component]
fn BoardLayout(id: u8) -> Element {
    rsx! {
        SuspenseBoundary {
            fallback: |_| rsx! {
                TopBar {}
                Loading {}
            },
            BoardLoader { id }
        }
    }
}

#[component]
fn BoardLoader(id: u8) -> Element {
    let loaded = use_cached_model(1000 + u32::from(id))?;
    if loaded {
        let frame = rsx! { BoardFrame { key: "{id}", id } };
        rsx! { {frame} }
    } else {
        rsx! {
            TopBar {}
            Loading {}
        }
    }
}

#[component]
fn BoardFrame(id: u8) -> Element {
    let _ = ROUTE();
    let tick = FRAME_TICK();
    rsx! {
        TopBar {}
        div { class: "bw", "data-board": "{id}", "data-tick": "{tick}",
            Sidebar {}
            Dialog {}
            Dialog {}
            Dialog {}
            Dialog {}
            Dialog {}
            main { class: "bw-main", "data-tick": "{tick}",
                SuspenseBoundary {
                    fallback: |_| rsx! {
                        div { class: "bw-scroll", Loading {} }
                    },
                    BoardOutlet {}
                }
            }
        }
    }
}

#[component]
fn Sidebar() -> Element {
    let route = ROUTE();
    let tick = SIDE_TICK();
    rsx! { aside { class: "bw-side", "data-tick": "{tick}", "{route:?}" } }
}

#[component]
fn Dialog() -> Element {
    rsx! { div { "popover": "auto", "dialog" } }
}

#[component]
fn BoardOutlet() -> Element {
    let (_, place) = ROUTE();
    match place {
        Place::Feed => rsx! {},
        Place::Overview => rsx! { Overview {} },
        Place::Space(space) => rsx! { SpacePage { space } },
        Place::Item(number) => rsx! { WorkItemPage { number } },
        Place::Page(page) => rsx! { PageView { page } },
    }
}

#[component]
fn PaneHeader(label: String) -> Element {
    rsx! { div { class: "bw-head", "{label}" } }
}

#[component]
fn Overview() -> Element {
    rsx! {
        Empty {}
        PaneHeader { label: "overview" }
        div { class: "bw-scroll", "overview" }
    }
}

#[component]
fn SpacePage(space: u8) -> Element {
    let items = rsx! { WorkItems { key: "{space}", space } };
    rsx! { {items} }
}

#[component]
fn WorkItems(space: u8) -> Element {
    if use_cached_model(2000 + u32::from(space))? {
        let view = rsx! { ItemsView { key: "{space}", space } };
        rsx! { {view} }
    } else {
        rsx! {
            PaneHeader { label: "items" }
            div { class: "bw-scroll", Loading {} }
        }
    }
}

#[component]
fn ItemsView(space: u8) -> Element {
    rsx! {
        Empty {}
        PaneHeader { label: "items {space}" }
        div { class: "bw-scroll", "list {space}" }
    }
}

#[component]
fn WorkItemPage(number: u8) -> Element {
    if use_cached_model(3000 + u32::from(number))? {
        let view = rsx! { ItemView { key: "{number}", number } };
        rsx! { {view} }
    } else {
        rsx! {
            PaneHeader { label: "#{number}" }
            div { class: "bw-scroll", Loading {} }
        }
    }
}

#[component]
fn ItemView(number: u8) -> Element {
    let trashed = number % 2 == 1;
    rsx! {
        Empty {}
        PaneHeader { label: "#{number}" }
        if trashed {
            div { class: "banner", "trashed" }
        }
        div { class: "bw-scroll wi-detail", "item {number}" }
    }
}

#[component]
fn PageView(page: u8) -> Element {
    if use_cached_model(4000 + u32::from(page))? {
        let view = rsx! { PageBody { key: "{page}", page } };
        rsx! { {view} }
    } else {
        rsx! {
            PaneHeader { label: "page" }
            div { class: "bw-scroll", Loading {} }
        }
    }
}

#[component]
fn PageBody(page: u8) -> Element {
    rsx! {
        Empty {}
        PaneHeader { label: "page {page}" }
        div { class: "bw-scroll", "page {page}" }
    }
}

/// Like `Loading`: an insect that re-renders itself while it's up.
#[component]
fn Loading() -> Element {
    rsx! {
        div { class: "loading",
            Insect {}
            p { "Loading…" }
        }
    }
}

#[component]
fn Insect() -> Element {
    let step = INSECT();
    rsx! { span { "step {step}" } }
}

/// A small deterministic generator (xorshift), so a failing seed replays.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}

fn random_place(rng: &mut Rng) -> Place {
    match rng.below(10) {
        0 => Place::Feed,
        1 => Place::Overview,
        2..=3 => Place::Space(rng.below(3) as u8),
        4..=7 => Place::Item(rng.below(4) as u8),
        _ => Place::Page(rng.below(3) as u8),
    }
}

/// Runs one seed's random session; on a divergence, says which seed and the steps that led
/// there.
async fn run_seed(seed: u64, steps: usize) -> Result<(), String> {
    CACHE.with(|cache| cache.borrow_mut().clear());
    PENDING.with(|pending| pending.borrow_mut().clear());
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut log = Vec::new();

    let mut dom = VirtualDom::new(app);
    let mut oracle = RendererOracle::new();
    oracle.rebuild(&mut dom);
    let check = |oracle: &RendererOracle, dom: &VirtualDom, log: &[String]| {
        oracle
            .check_matches_vdom(dom)
            .map_err(|err| format!("seed {seed}, after {log:#?}\n{err}"))
    };

    for _ in 0..steps {
        let step = match rng.below(11) {
            0..=2 => {
                let (board, _) = dom.in_scope(ScopeId::APP, || *ROUTE.peek());
                let place = random_place(&mut rng);
                dom.in_scope(ScopeId::APP, || *ROUTE.write() = (board, place));
                format!("go {place:?}")
            }
            3 => {
                let board = rng.below(2) as u8;
                let place = match random_place(&mut rng) {
                    Place::Feed => Place::Overview,
                    place => place,
                };
                dom.in_scope(ScopeId::APP, || *ROUTE.write() = (board, place));
                format!("board {board} {place:?}")
            }
            4 => {
                dom.in_scope(ScopeId::APP, || *FRAME_TICK.write() += 1);
                "frame".to_owned()
            }
            5 => {
                dom.in_scope(ScopeId::APP, || *SHELL_TICK.write() += 1);
                "shell".to_owned()
            }
            6 => {
                dom.in_scope(ScopeId::APP, || *SIDE_TICK.write() += 1);
                "side".to_owned()
            }
            7 => {
                dom.in_scope(ScopeId::APP, || *INSECT.write() += 1);
                "insect".to_owned()
            }
            _ => {
                // Finish one load, or all of them, and render what that wakes.
                let all = rng.below(2) == 0;
                let released = PENDING.with(|pending| {
                    let mut pending = pending.borrow_mut();
                    if all {
                        pending.drain(..).fold(0, |sent, release| {
                            sent + usize::from(release.send(()).is_ok())
                        })
                    } else if pending.is_empty() {
                        0
                    } else {
                        let at = rng.below(pending.len() as u64) as usize;
                        usize::from(pending.remove(at).send(()).is_ok())
                    }
                });
                while tokio::time::timeout(Duration::from_millis(2), dom.wait_for_work())
                    .await
                    .is_ok()
                {
                    oracle.render(&mut dom);
                    check(&oracle, &dom, &log)?;
                }
                format!("finish {} ({released})", if all { "all" } else { "one" })
            }
        };
        log.push(step);
        // Sometimes the next change lands before this one is rendered.
        if rng.below(4) != 0 {
            log.push("render".to_owned());
            oracle.render(&mut dom);
            check(&oracle, &dom, &log)?;
        }
    }
    Ok(())
}

#[test]
fn random_sessions_keep_pages_in_their_pane() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    let seeds: u64 = std::env::var("SEEDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(200);
    let first: u64 = std::env::var("FIRST_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    for seed in first..first + seeds {
        eprintln!("seed {seed}");
        if let Err(err) = runtime.block_on(run_seed(seed, 60)) {
            panic!("{err}");
        }
    }
}

/// Steps through `steps` from a todo list that's kept, with loads that never finish.
async fn replay(steps: &[&str]) -> Result<(), String> {
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.clear();
        cache.extend([1000, 2000]);
    });
    PENDING.with(|pending| pending.borrow_mut().clear());
    let mut dom = VirtualDom::new(|| {
        rsx! { div { class: "app", Shell {} } }
    });
    let mut oracle = RendererOracle::new();
    oracle.rebuild(&mut dom);
    dom.in_scope(ScopeId::APP, || *ROUTE.write() = (0, Place::Space(0)));
    oracle.render(&mut dom);
    oracle.check_matches_vdom(&dom)?;
    for (i, step) in steps.iter().enumerate() {
        dom.in_scope(ScopeId::APP, || match *step {
            "frame" => *FRAME_TICK.write() += 1,
            "space0" => ROUTE.write().1 = Place::Space(0),
            "space1" => ROUTE.write().1 = Place::Space(1),
            "item" => ROUTE.write().1 = Place::Item(2),
            "overview" => ROUTE.write().1 = Place::Overview,
            other => panic!("no step {other}"),
        });
        oracle.render(&mut dom);
        oracle
            .check_matches_vdom(&dom)
            .map_err(|err| format!("step {i} ({step}) of {steps:?}\n{err}"))?;
    }
    Ok(())
}

/// Waiting again after a wait that ended (by going back to something kept) keeps the pane's
/// fallback and children in the pane.
#[test]
fn a_boundary_that_waits_again_stays_in_place() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    for steps in [
        &["space1", "frame", "space0", "item"][..],
        &["space1", "space0", "item"],
        &["space1", "frame", "space0"],
        &["space1", "item"],
        &["item", "space0", "item"],
        &["space1", "space0", "space1"],
    ] {
        if let Err(err) = runtime.block_on(replay(steps)) {
            panic!("{err}");
        }
    }
}

#[component]
fn Forever() -> Element {
    let task = use_hook(|| spawn(std::future::pending::<()>()));
    suspend(task)?;
    rsx! { "never" }
}

async fn fallback_steps(app: fn() -> Element) -> Result<(), String> {
    let mut dom = VirtualDom::new(app);
    let mut oracle = RendererOracle::new();
    oracle.rebuild(&mut dom);
    oracle.check_matches_vdom(&dom)?;
    dom.in_scope(ScopeId::APP, || *INSECT.write() += 1);
    oracle.render(&mut dom);
    oracle.check_matches_vdom(&dom)
}

/// A named app.
type Case = (&'static str, fn() -> Element);

/// A fallback's own components keep re-rendering to the renderer while their boundary waits.
#[test]
fn fallbacks_update_while_their_boundary_waits() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    let cases: [Case; 3] = [
        ("one boundary", || {
            rsx! {
                SuspenseBoundary { fallback: |_| rsx! { Loading {} }, Forever {} }
            }
        }),
        ("fallback with two roots", || {
            rsx! {
                SuspenseBoundary { fallback: |_| rsx! { TopBar {} Loading {} }, Forever {} }
            }
        }),
        ("nested in a resting boundary", || {
            rsx! {
                SuspenseBoundary { fallback: |_| rsx! { "outer" },
                    SuspenseBoundary { fallback: |_| rsx! { TopBar {} Loading {} }, Forever {} }
                }
            }
        }),
    ];
    for (name, app) in cases {
        if let Err(err) = runtime.block_on(fallback_steps(app)) {
            panic!("{name}: {err}");
        }
    }
}
