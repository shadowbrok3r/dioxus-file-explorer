use dioxus::prelude::*;
use std::collections::{HashMap, BTreeMap};

pub fn render_icons(
	props: crate::components::results_component::ResultsProps,
	_collapsed: Signal<HashMap<String, bool>>,
	enriched: Vec<crate::utilities::types::FileRecord>,
	_grouped: Option<BTreeMap<String, Vec<crate::utilities::types::FileRecord>>>,
) -> Element {
	let mut selected_path = props.selected_path;
	let mut selected_paths = props.selected_paths;
	let all_cached = props.all_cached;

	rsx! {
		div { class: "flex flex-wrap gap-3",
			{ enriched.iter().map(|rec| {
				let fname = rec.path.file_name().and_then(|f| f.to_str()).unwrap_or("").to_string();
				let is_sel = selected_path.read().as_ref().map(|p| p == &rec.path).unwrap_or(false)
					|| selected_paths.read().contains(&rec.path);
				let mut card_cls = "file-card group p-2 rounded-md border flex flex-col gap-2 cursor-pointer select-none w-[132px]".to_string();
				if is_sel { card_cls.push_str(" is-selected"); }
				let key = rec.path.display().to_string();
				let inline_thumb = rec.thumb_data.clone();
				let cached_thumb = all_cached.read().get(&key).and_then(|t| t.thumbnail_b64.clone());
				let thumb_b64 = inline_thumb.or(cached_thumb);
				let rec_path_click = rec.path.clone();
				let rec_path_key = rec.path.clone();
				rsx! {
					div { key: "{key}", class: "{card_cls}", tabindex: 0,
						onclick: move |_| {
							selected_path.set(Some(rec_path_click.clone()));
							let mut set = selected_paths.write();
							set.clear();
							set.insert(rec_path_click.clone());
						},
						onkeydown: move |e| {
							let k = e.key().to_string();
							if k == "Enter" || k == " " || k == "Space" || k == "Spacebar" {
								e.prevent_default();
								selected_path.set(Some(rec_path_key.clone()));
								let mut set = selected_paths.write();
								set.clear();
								set.insert(rec_path_key.clone());
							}
						},
						div { 
							class: "relative w-full aspect-square rounded-md overflow-hidden file-card-thumb flex items-center justify-center",
							"data-style": "glass", 
							if let Some(ref b64) = thumb_b64 { img { class: "w-full h-full object-cover object-center pointer-events-none select-none", draggable: "false", alt: "{fname}", src: "data:image/*;base64,{b64}" } }
							if thumb_b64.is_none() { i { class: "material-icons text-[40px] opacity-60", "insert_drive_file" } }
						}
						span { class: "block text-11px leading-tight font-medium truncate", title: "{fname}", "{fname}" }
					}
				}
			}) }
		}
	}
}


