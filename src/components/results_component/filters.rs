fn type_filter_dropdown(ext_filters: Signal<std::collections::BTreeSet<String>>, mut ext_enabled: Signal<std::collections::BTreeMap<String,bool>>, disabled_ct: usize) -> Element {
    let type_nodes = rsx! {
        for ename in ext_filters.read().iter().cloned() {
            {
                let active = *ext_enabled.read().get(&ename).unwrap_or(&true);
                let state_cls = if active { "filter-btn-active" } else { "filter-btn-inactive" };
                rsx! {
                    button {
                        key: "{ename}",
                        "data-style": "outline",
                        class: "button text-10px flex justify-between {state_cls}",
                        onclick: move |_| {
                            let mut map = ext_enabled.write();
                            let cur = map.get(&ename).cloned().unwrap_or(true);
                            map.insert(ename.clone(), !cur);
                        },
                        span { ".{ename}" }
                        span { class: "material-icons", {if active { "check" } else { "close" }} }
                    }
                }
            }
        }
    };
    rsx! {
        DropdownMenu { class: "inline-block",
            DropdownMenuTrigger {
                "data-style": "glass",
                class: "button px-1 py-0 h-6 text-[10px] leading-none flex items-center gap-0.5 relative filter-trigger",
                i { class: "material-icons text-[16px] opacity-80", "filter_list" }
                if disabled_ct > 0 {
                    span { class: "filter-badge", "{disabled_ct}" }
                }
            }
            DropdownMenuContent { class: "menubar-content flex flex-col gap-2 min-w-[180px] filter-dropdown-list",
                if ext_filters.read().is_empty() {
                    span { class: "text-10px text-weak", "No types" }
                }
                if !ext_filters.read().is_empty() {
                    div { class: "filter-dropdown-list", {type_nodes} }
                    button {
                        class: "filter-reset-link",
                        onclick: move |_| {
                            let exts: Vec<String> = ext_filters.read().iter().cloned().collect();
                            let mut map = ext_enabled.write();
                            for e in exts {
                                map.insert(e, true);
                            }
                        },
                        "Enable All"
                    }
                }
            }
        }
    }
}
