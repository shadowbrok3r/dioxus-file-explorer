use dioxus::prelude::*;
use crate::utilities::types::ScanResults;

#[derive(Props, PartialEq, Clone)]
pub struct ProgressOverlayProps {
    pub progress: Signal<Option<(usize,usize)>>,
    pub scanning: Signal<bool>,
    pub recursive_current: Signal<bool>,
    pub scan_started: Signal<Option<std::time::Instant>>,
    pub scan_finished: Signal<Option<std::time::Instant>>,
    pub results: Signal<ScanResults>,
    // bulk generation (AI description) progress (done,total)
    pub bulk_progress: Signal<(usize,usize)>,
    pub bulk_generating: Signal<bool>,
    // expansion control
    pub show_expanded: Signal<bool>,
    // action callbacks (provided by parent) kept simple for now
    pub on_select_all: EventHandler<()>,
    pub on_filter_images: EventHandler<()>,
    pub on_filter_videos: EventHandler<()>,
    pub on_filter_all: EventHandler<()>,
    pub on_sort_name: EventHandler<()>,
    pub on_sort_date: EventHandler<()>,
    pub on_sort_size: EventHandler<()>,
    // initial position optional in future
}

#[allow(non_snake_case)]
#[component]
pub fn ProgressOverlay(props: ProgressOverlayProps) -> Element {
    let ProgressOverlayProps { progress, scanning, recursive_current, scan_started, scan_finished, results, bulk_progress, bulk_generating, mut show_expanded, on_select_all, on_filter_images, on_filter_videos, on_filter_all, on_sort_name, on_sort_date, on_sort_size } = props;

    // Removed drag support; fixed centered positioning handled via style

    // Current progress tuple (scanned, total). For recursive scans total==0 means unknown.
    let prog = progress.read().clone().map(|(s,t)| if t>0 { (s.min(t), t) } else { (s,t) });
    let started_opt = scan_started.read().clone();
    let finished_opt = scan_finished.read().clone();
    let elapsed = match (started_opt, finished_opt) { (Some(st), Some(fin)) => fin.duration_since(st), (Some(st), None) => st.elapsed(), _ => std::time::Duration::default() };
    let secs = elapsed.as_secs_f32();
    let done = !*scanning.read();
    let found_count = results.read().items.len();
    let zero_media_done = done && found_count == 0; // differentiate directory-only/empty results
    let expanded = *show_expanded.read();
    let container_classes = if expanded { "progress-overlay-btn bg-muted overflow-hidden flex flex-col border shadow-xl" } else { "bg-muted overflow-hidden flex flex-col border" };
    let style_pos = "position:fixed; left:50%; transform:translateX(-50%); bottom:12px; border-radius:10px; min-width:320px; border-color: var(--error); z-index:120;";
    rsx! {
        div { class: "{container_classes}", style: "{style_pos}",
            // progress bar region (using built-in progress element)
            div {
                class: "w-full h-1 relative overflow-hidden bg-muted/60",
                style: "border-top-left-radius:10px;border-top-right-radius:10px;",
                {
                    if !zero_media_done {
                        match prog {
                            Some((scanned, total)) if total > 0 => {
                                let val_current = scanned.min(total);
                                rsx! {
                                    progress {
                                        value: val_current,
                                        max: total,
                                        class: "w-full h-1 appearance-none [&::-webkit-progress-bar]:bg-transparent [&::-webkit-progress-value]:bg-accent [&::-moz-progress-bar]:bg-accent transition-[width] duration-150",
                                    }
                                }
                            }
                            Some((_scanned, _total)) => rsx! {
                                progress { class: "w-full h-1 indeterminate-progress" }
                            },
                            None => rsx! {
                                progress { class: "w-full h-1 indeterminate-progress" }
                            },
                        }
                    } else {
                        rsx! { progress { class: "w-full h-1 indeterminate-progress" } } 
                    }
                }
                if done {
                    div {
                        class: "absolute inset-0 mix-blend-multiply pointer-events-none",
                        style: "opacity:0.35;",
                    }
                }
            }
            div {
                class: "flex flex-wrap gap-3 px-3 py-2 text-11px text-weak items-center select-none",
                style: "user-select:none;",
                i { class: "material-icons text-sm opacity-70", "open_with" }
                if !zero_media_done {
                    span {
                        class: "px-1.5 py-0.5 rounded-full text-10px tracking-wide uppercase font-medium ",
                        class: if done { "scanning-done" } else { "scanning" },
                        {
                            if done {
                                "Done"
                            } else if *recursive_current.read() {
                                "Deep"
                            } else {
                                "Shallow"
                            }
                        }
                    }
                } else {
                    span { class: "px-1.5 py-0.5 rounded-full text-10px tracking-wide uppercase font-medium bg-transparent text-weak border border-dashed border-stroke", "Idle" }
                }
                if zero_media_done {
                    span { class: "italic opacity-70", "No media files" }
                } else {
                    if let Some((scanned, total)) = prog {
                        if total > 0 {
                            {
                                let pct = scanned as f32 * 100.0 / total.max(1) as f32;
                                let pct_rounded = pct.round() as i32;
                                let rate = if secs > 0.25 { scanned as f32 / secs } else { 0.0 };
                                rsx! {
                                    span { "{scanned} / {total} ({pct_rounded}%)" }
                                    span { "found {found_count}" }
                                    if done {
                                        span { "in {secs:.1}s" }
                                    }
                                    if !done && rate > 0.1 {
                                        span { "{rate:.1} items/s" }
                                    }
                                }
                            }
                        } else {
                            {
                                let rate = if secs > 0.25 { scanned as f32 / secs } else { 0.0 };
                                rsx! {
                                    span { "{scanned} scanned" }
                                    span { "found {found_count}" }
                                    if !done && rate > 0.1 {
                                        span { "{rate:.1} items/s" }
                                    }
                                    span { {if done { format!("in {:.1}s", secs) } else { "estimating...".to_string() }} }
                                }
                            }
                        }
                    } else {
                        span {
                            if done {
                                "No items"
                            } else {
                                "Starting scan..."
                            }
                        }
                    }
                }
                // bulk generation inline status (only if active or some progress)
                {
                    let (bd, bt) = *bulk_progress.read();
                    if *bulk_generating.read() || bt > 0 {
                        let pct = if bt > 0 {
                            (bd as f32 * 100.0 / bt as f32).round() as i32
                        } else {
                            0
                        };
                        rsx! {
                            span { class: "ml-1 px-1 py-0.5 rounded-full bg-accent/20 text-accent text-9px",
                                {if bt > 0 { format!("Desc {bd}/{bt} ({pct}%)") } else { "Preparing...".into() }}
                            }
                        }
                    } else {
                        rsx! {}
                    }
                }
                button {
                    class: "btn ml-auto rounded-full transition p-1",
                    title: if expanded { "Collapse" } else { "Expand" },
                    onclick: move |_| {
                        let cur = *show_expanded.read();
                        show_expanded.set(!cur);
                    },
                    i { class: "material-icons",
                        {if expanded { "keyboard_arrow_down" } else { "keyboard_arrow_up" }}
                    }
                }
            }
            if expanded {
                div { class: "px-2 pb-2 flex flex-col gap-2 border-t border-stroke bg-panel/60 backdrop-blur-sm",
                    // actions row 1
                    div { class: "flex gap-2 flex-wrap",
                        button {
                            class: "btn",
                            onclick: move |_| on_select_all.call(()),
                            "Select All"
                        }
                        button {
                            class: "btn",
                            onclick: move |_| on_filter_images.call(()),
                            "Images"
                        }
                        button {
                            class: "btn",
                            onclick: move |_| on_filter_videos.call(()),
                            "Videos"
                        }
                        button {
                            class: "btn",
                            onclick: move |_| on_filter_all.call(()),
                            "All"
                        }
                    }
                    // Bulk generation progress detail (if any)
                    {
                        let (bd, bt) = *bulk_progress.read();
                        if *bulk_generating.read() || bt > 0 {
                            let pct = if bt > 0 {
                                (bd as f32 * 100.0 / bt as f32).round() as i32
                            } else {
                                0
                            };
                            rsx! {
                                div { class: "flex items-center gap-2 text-10px text-weak",
                                    span { "Descriptions: {bd} / {bt} ({pct}%)" }
                                    {
                                        if *bulk_generating.read() {
                                            rsx! {
                                                span { class: "animate-pulse text-accent", "Generating..." }
                                            }
                                        } else if bt > 0 && bd >= bt {
                                            rsx! {
                                                span { class: "text-green-500", "Done" }
                                            }
                                        } else {
                                            rsx! {}
                                        }
                                    }
                                }
                            }
                        } else {
                            rsx! {}
                        }
                    }
                    // sorting
                    div { class: "flex gap-2 flex-wrap",
                        span { class: "text-8px uppercase tracking-wide text-weak", "Sort:" }
                        button {
                            class: "btn",
                            onclick: move |_| on_sort_name.call(()),
                            "Name"
                        }
                        button {
                            class: "btn",
                            onclick: move |_| on_sort_date.call(()),
                            "Date"
                        }
                        button {
                            class: "btn",
                            onclick: move |_| on_sort_size.call(()),
                            "Size"
                        }
                    }
                    // placeholder for columns config
                    div { class: "flex gap-2 flex-wrap text-8px text-weak",
                        span { "Columns config coming soon..." }
                    }
                }
            }
        }
    }
}

// simple utility styling classes may be defined in global css: .btn
