use dioxus::prelude::*;
use humansize::{format_size, DECIMAL};
use std::path::PathBuf;

#[derive(Props, PartialEq, Clone)]
pub struct LeftSidebarProps {
    pub filters: Signal<crate::utilities::types::Filters>,
    pub qa_collapsed: Signal<bool>,
    pub drives_collapsed: Signal<bool>,
    pub ui: Signal<crate::settings::UiSettings>,
    pub left_width: Signal<u32>,
    pub resizing_left: Signal<Option<(i32,u32)>>,
    pub path_text: Signal<String>,
    pub recursive_current: Signal<bool>,
    pub only_subdirs: Signal<bool>,
    pub scan_started: Signal<Option<std::time::Instant>>,
    pub results: Signal<crate::utilities::types::ScanResults>,
    pub dir_items: Signal<Vec<crate::utilities::types::DirItem>>,
}

#[allow(non_snake_case)]
pub fn LeftSidebar(props: LeftSidebarProps) -> Element {
    let mut filters = props.filters;
    let mut qa_collapsed = props.qa_collapsed;
    let mut drives_collapsed = props.drives_collapsed;
    let mut ui = props.ui;
    let mut left_width = props.left_width; // needs &mut for .set
    let mut resizing_left = props.resizing_left;
    let mut path_text = props.path_text;
    let mut recursive_current = props.recursive_current;
    let mut only_subdirs = props.only_subdirs;
    let mut scan_started = props.scan_started;
    let mut results = props.results;
    let dir_items = props.dir_items;

    let computed_width = if *qa_collapsed.read() && *drives_collapsed.read() { 14 } else { (*left_width.read()).max(180).min(480) };

    rsx! {
        aside {
            class: "panel p-2",
            "data-style": "outline",
            style: "user-select: none; width: {computed_width}px; overflow-y:auto; overflow-x: hidden; transition: width .08s ease; position:relative; border-radius:16px",
            // Pointer events instead of web-only document listeners (desktop build)
            onpointermove: move |evt| {
                if let Some((start_x, start_w)) = *resizing_left.read() {
                    let dx = evt.client_coordinates().x as i32 - start_x;
                    let new_w = (start_w as i32 + dx).max(140).min(640) as u32;
                    left_width.set(new_w);
                }
            },
            onpointerup: move |_| {
                if resizing_left.read().is_some() {
                    resizing_left.set(None);
                    let mut s = ui.write();
                    s.left_width = *left_width.read();
                    crate::settings::save_settings(&s);
                }
            },
            // Quick Access header
            div { class: "flex items-center justify-between px-3 py-2 border-b",
                h3 { class: "text-18px font-semibold", "Quick Access" }
                button {
                    class: "button",
                    "data-style": "glass",
                    onclick: move |_| {
                        let curr = *qa_collapsed.read();
                        qa_collapsed.set(!curr);
                        let mut s = ui.write();
                        s.qa_collapsed = !curr;
                        crate::settings::save_settings(&s);
                    },
                    i { class: "material-icons",
                        {if *qa_collapsed.read() { "chevron_right" } else { "expand_more" }}
                    }
                }
            }
            if !*qa_collapsed.read() {
                ul {
                    class: "qa-list",
                    style: "padding:10px 12px; display:grid; gap:8px;",
                    for qa in crate::utilities::explorer::quick_access().into_iter() {
                        {
                            let label = qa.label.clone();
                            let path = qa.path.clone();
                            rsx! {
                                li {
                                    key: "{label}",
                                    class: "panel",
                                    "data-style": "glass",
                                    "data-accent": "secondary",
                                    style: "padding-left:14px;",
                                    onclick: move |_| {
                                        let new_root = path.clone();
                                        {
                                            let mut f = filters.write();
                                            f.root = new_root.clone();
                                        }
                                        path_text.set(new_root.display().to_string());
                                        recursive_current.set(false);
                                        if crate::app::shallow_should_scan(&new_root) {
                                            only_subdirs.set(false);
                                            scan_started.set(Some(std::time::Instant::now()));
                                        } else {
                                            only_subdirs.set(true);
                                            let mut dir_items_sig = dir_items.clone();
                                            let root_for_list = new_root.clone();
                                            dioxus::prelude::spawn(async move {
                                                match crate::utilities::explorer::list_dir_items(root_for_list).await {
                                                    Ok(items) => dir_items_sig.set(items),
                                                    Err(_) => dir_items_sig.set(Vec::new()),
                                                }
                                            });
                                            results.set(Default::default());
                                        }
                                    },
                                    i { class: "material-icons",
                                        match label.as_str() {
                                            "Pictures" => "image",
                                            "Videos" => "video_call",
                                            "Desktop" => "desktop_windows",
                                            "Documents" => "article",
                                            "Downloads" => "download",
                                            "Home" => "home",
                                            _ => "star",
                                        }
                                    }
                                    span { class: "items-center", style: "margin: auto 10px", "{label}" }
                                }
                            }
                        }
                    }
                }
            }
            // Drives header
            div { class: "flex items-center justify-between px-3 py-2 border-t border-b",
                h3 { class: "text-18px font-semibold", "Drives" }
                button {
                    class: "button",
                    "data-style": "glass",
                    onclick: move |_| {
                        let curr = *drives_collapsed.read();
                        drives_collapsed.set(!curr);
                        let mut s = ui.write();
                        s.drives_collapsed = !curr;
                        crate::settings::save_settings(&s);
                    },
                    i { class: "material-icons",
                        {if *drives_collapsed.read() { "chevron_right" } else { "expand_more" }}
                    }
                }
            }
            if !*drives_collapsed.read() {
                ul {
                    class: "qa-list",
                    style: "padding:10px 12px; display:grid; gap:8px;",
                    for info in crate::utilities::explorer::list_drive_infos().into_iter() {
                        {
                            let root = info.root.clone();
                            let display = if info.label.is_empty() {
                                info.root.clone()
                            } else {
                                format!("{} ({})", info.root, info.label)
                            };
                            let path = PathBuf::from(root.clone());
                            let free = format_size(info.free, DECIMAL);
                            let total = format_size(info.total, DECIMAL);
                            rsx! {
                                li {
                                    key: "{display}",
                                    class: "panel",
                                    "data-style": "glass",
                                    "data-accent": "tertiary",
                                    style: "display:grid; grid-template-columns:24px 1fr; align-items:center; gap:8px; padding:8px 14px 8px 16px;",
                                    onclick: move |_| {
                                        let new_root = path.clone();
                                        {
                                            let mut f = filters.write();
                                            f.root = new_root.clone();
                                        }
                                        path_text.set(new_root.display().to_string());
                                        recursive_current.set(false);
                                        if crate::app::shallow_should_scan(&new_root) {
                                            only_subdirs.set(false);
                                            scan_started.set(Some(std::time::Instant::now()));
                                        } else {
                                            only_subdirs.set(true);
                                            let mut dir_items_sig = dir_items.clone();
                                            let root_for_list = new_root.clone();
                                            dioxus::prelude::spawn(async move {
                                                match crate::utilities::explorer::list_dir_items(root_for_list).await {
                                                    Ok(items) => dir_items_sig.set(items),
                                                    Err(_) => dir_items_sig.set(Vec::new()),
                                                }
                                            });
                                            results.set(Default::default());
                                        }
                                    },
                                    i { class: "material-icons", style: "font-size:20px;",
                                        {crate::utilities::explorer::drive_icon_for_root(&root)}
                                    }
                                    div { class: "flex flex-col min-w-0",
                                        span {
                                            title: "{display}",
                                            style: "white-space:nowrap; overflow:hidden; text-overflow:ellipsis; font-size:13px;",
                                            "{display}"
                                        }
                                        // Progress {
                                        //     class: "progress",
                                        //     value: progress() as f64,
                                        //     ProgressIndicator { class: "progress-indicator" }
                                        // }
                                        span {
                                            class: "text-weak text-8px",
                                            style: "white-space:nowrap; overflow:hidden; text-overflow:ellipsis;",
                                            "{free} free of {total}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if *qa_collapsed.read() && *drives_collapsed.read() {
                div {
                    style: "position:absolute; inset:0; z-index:10; cursor:pointer;",
                    onclick: move |_| {
                        qa_collapsed.set(false);
                        drives_collapsed.set(false);
                        let mut s = ui.write();
                        s.qa_collapsed = false;
                        s.drives_collapsed = false;
                        crate::settings::save_settings(&s);
                    },
                }
                div { style: "position:absolute; top:8px; left:2px; width:20px; height:20px; z-index:11;",
                    button {
                        class: "",
                        "data-style": "outline",
                        title: "Expand side panel",
                        onclick: move |_| {
                            qa_collapsed.set(false);
                            let mut s = ui.write();
                            s.qa_collapsed = false;
                            crate::settings::save_settings(&s);
                        },
                        i { class: "material-icons", "chevron_right" }
                    }
                }
            }
            // Resize handle
            div { class: "resize-handle", style: "position:absolute; top:0; right:-3px; width:5px; height:100%; cursor: ew-resize;",
                onpointerdown: move |evt| { resizing_left.set(Some((evt.client_coordinates().x as i32, *left_width.read()))); }
            }
        }
    }
}
