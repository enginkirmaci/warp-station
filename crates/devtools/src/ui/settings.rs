//! The Settings page: every settings section as a card of labeled inputs and
//! switches, seeded from the live settings when the page opens. Save writes
//! the whole graph atomically and live-applies (a scan-related change also
//! kicks a rescan). The filtered model pickers list the opencode catalog with a
//! refresh button, and the AI features card manages the wand prompt templates.

use crate::net::TemplateOp;
use crate::store::Store;
use crate::theme;
use warpui_kit_markdown::{live_highlighter_factory, LIVE_LANGUAGE};
use warpui_kit_picker::{
    model_name, model_provider, FilteredPicker, OverlayAnchor, PanelChrome, PickerBody, PickerData,
    PickerIcons, PickerItem, PickerRow, PickerSpec, PickerTarget, Selection, TriggerScale,
};
use crate::ui::A11y;
use gpui::prelude::*;
use gpui::{div, px, relative, Context, Entity, FontWeight, SharedString, Window};
use gpui_kit::component::input::{
    Editor, EditorState, Input, InputState, Textarea, TextareaState,
};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::switch::Switch;
use std::cell::RefCell;
use std::rc::Rc;

pub struct SettingsPage {
    pub store: Entity<Store>,
    // General
    notes_store_path: Entity<InputState>,
    show_tooltips: bool,
    start_minimized: bool,
    start_at_boot: bool,
    text_size: &'static str,
    // Scanning
    scan_folders: Entity<TextareaState>,
    excluded_folders: Entity<TextareaState>,
    git_folder_pattern: Entity<InputState>,
    solution_file_pattern: Entity<InputState>,
    platform_folder_name: Entity<InputState>,
    max_scan_depth: Entity<InputState>,
    group_by_root: bool,
    group_depth: usize,
    // Git
    branch_name_prefix: Entity<InputState>,
    auto_fetch_minutes: Option<u16>,
    auto_fetch_only_focused: bool,
    // External tools
    vscode_executable: Entity<InputState>,
    vscode_profile: Entity<InputState>,
    terminal_executable: Entity<InputState>,
    ide_executable: Entity<InputState>,
    enable_code_search: bool,
    code_search_executable: Entity<InputState>,
    enable_terminal: bool,
    enable_vscode: bool,
    enable_visual_studio: bool,
    snap_it_auto_start: bool,
    snap_it_executable: Entity<InputState>,
    keep_awake_enabled: bool,
    // OpenCode CLI (the repo row's launch button)
    enable_opencode: bool,
    opencode_executable: Entity<InputState>,
    // AI features (warp-agent)
    warp_enabled: bool,
    warp_executable: Entity<InputState>,
    // Notes (warp-notes)
    warp_notes_enabled: bool,
    warp_notes_executable: Entity<InputState>,
    default_model: Entity<InputState>,
    commit_model: Entity<InputState>,
    default_model_picker: Entity<FilteredPicker>,
    commit_model_picker: Entity<FilteredPicker>,
    // The template being edited inline (registry id); None = the rows list.
    editing_template: Option<&'static str>,
    /// True once the open editor's TemplateLoaded landed and seeded the buffer.
    template_editor_seeded: bool,
    /// The embedded markdown editor for the open template.
    template_editor: Entity<EditorState>,
    /// The row whose Reset is armed (a second click confirms; arming another
    /// row disarms this one).
    reset_armed: Option<&'static str>,
    // Columns
    enable_github: bool,
    github_executable: Entity<InputState>,
    enable_azure: bool,
    azure_pat: Entity<InputState>,
    azure_url: Entity<InputState>,
    saved_note: Option<std::time::Instant>,
    /// set by the header back button; MainWindow drains it and swaps pages
    pub back_requested: bool,
}

impl SettingsPage {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = store.read(cx).settings();
        let repos = &settings.repos;
        let general = &settings.general;
        let line_list = |items: &Option<Vec<String>>| {
            items.clone().unwrap_or_default().join("\n")
        };

        fn field(window: &mut Window, cx: &mut Context<SettingsPage>, value: &str) -> Entity<InputState> {
            let initial = if value.is_empty() { String::new() } else { value.to_string() };
            cx.new(|cx| InputState::new(window, cx).default_value(initial))
        }
        fn area(window: &mut Window, cx: &mut Context<SettingsPage>, value: &str) -> Entity<TextareaState> {
            cx.new(|cx| TextareaState::new(window, cx).default_value(value.to_string()))
        }

        // the prompt-template editor mirrors the Notes live editor: markdown
        // live highlighting with caret-driven marker fading
        let template_caret = Rc::new(RefCell::new(Some(0..0)));
        let highlighter_caret = template_caret.clone();
        let template_editor = cx.new(|cx| {
            let mut state = EditorState::new(window, cx)
                .language(LIVE_LANGUAGE)
                .line_number(false)
                .folding(false)
                .auto_close(false)
                .smart_indent(false)
                .placeholder("loading…");
            state.set_highlighter_factory(live_highlighter_factory(highlighter_caret), cx);
            state
        });
        let observe_caret = template_caret.clone();
        cx.observe(&template_editor, move |_, editor, cx| {
            *observe_caret.borrow_mut() = Some(editor.read(cx).selected_range());
        })
        .detach();
        // settled TemplateIo ops land on the store; the template rows and
        // the open editor redraw from them
        cx.observe(&store, |_, _, cx| cx.notify()).detach();

        let default_model = field(window, cx, &settings.opencode.default_model);
        let commit_model = field(window, cx, &settings.opencode.commit_model.clone().unwrap_or_default());
        let default_model_picker = model_picker(
            &default_model,
            &store,
            "Default",
            "OpenCode default",
            window,
            cx,
        );
        let commit_model_picker = model_picker(
            &commit_model,
            &store,
            "Commit",
            "Use default model",
            window,
            cx,
        );

        let page = Self {
            store: store.clone(),
            notes_store_path: cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(general.notes_store_path.clone())
                    .placeholder(
                        devtools_core::notes::resolve_store_root("")
                            .to_string_lossy()
                            .to_string(),
                    )
            }),
            show_tooltips: general.show_tooltips,
            start_minimized: general.start_minimized,
            start_at_boot: general.start_at_boot,
            text_size: theme::normalize_font_scale(general.font_scale.as_deref()),
            scan_folders: area(window, cx, &line_list(&repos.repo_scan_folders)),
            excluded_folders: area(window, cx, &line_list(&repos.excluded_folders)),
            git_folder_pattern: field(window, cx, &repos.git_folder_pattern),
            solution_file_pattern: field(window, cx, &repos.solution_file_pattern),
            platform_folder_name: field(window, cx, &repos.platform_folder_name),
            max_scan_depth: field(window, cx, &repos.max_scan_depth.to_string()),
            group_by_root: repos.group_repos_by_root.unwrap_or(false),
            group_depth: repos.group_root_depth.unwrap_or(1).clamp(1, 4) as usize,
            branch_name_prefix: field(window, cx, &repos.branch_name_prefix),
            auto_fetch_minutes: repos.auto_fetch_minutes,
            auto_fetch_only_focused: repos.auto_fetch_only_focused.unwrap_or(false),
            vscode_executable: field(window, cx, &repos.vscode_executable),
            vscode_profile: field(window, cx, &repos.vscode_profile),
            terminal_executable: field(window, cx, &repos.terminal_executable),
            ide_executable: field(window, cx, &repos.ide_executable),
            opencode_executable: field(window, cx, &repos.opencode_executable),
            enable_code_search: repos.enable_code_search.unwrap_or(true),
            code_search_executable: field(
                window,
                cx,
                &repos.code_search_executable.clone().unwrap_or_else(|| "rg".into()),
            ),
            enable_terminal: repos.enable_terminal,
            enable_vscode: repos.enable_vscode,
            enable_visual_studio: repos.enable_visual_studio,
            snap_it_auto_start: settings.snap_it.auto_start,
            snap_it_executable: field(window, cx, &settings.snap_it.executable),
            keep_awake_enabled: settings.keep_awake.enabled,
            enable_opencode: settings.opencode.enable_opencode,
            warp_enabled: settings.warp_agent.enable_warp_agent,
            warp_executable: field(window, cx, &settings.warp_agent.executable),
            warp_notes_enabled: settings.warp_notes.enable_warp_notes,
            warp_notes_executable: field(window, cx, &settings.warp_notes.executable),
            default_model,
            commit_model,
            default_model_picker,
            commit_model_picker,
            editing_template: None,
            template_editor_seeded: false,
            template_editor,
            reset_armed: None,
            enable_github: repos.enable_git_hub,
            github_executable: field(window, cx, &repos.git_hub_executable),
            enable_azure: repos.enable_azure_dev_ops,
            azure_pat: field(window, cx, &repos.azure_dev_ops_pat),
            azure_url: field(window, cx, &repos.azure_dev_ops_url),
            saved_note: None,
            back_requested: false,
        };
        store.update(cx, |s, cx| s.load_models(cx));
        page
    }

    fn value(field: &Entity<InputState>, cx: &Context<Self>) -> String {
        field.read(cx).value().to_string()
    }

    fn lines(field: &Entity<TextareaState>, cx: &Context<Self>) -> Vec<String> {
        field
            .read(cx)
            .value()
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect()
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let scan_folders = Self::lines(&self.scan_folders, cx);
        let excluded = Self::lines(&self.excluded_folders, cx);
        let max_scan_depth = Self::value(&self.max_scan_depth, cx)
            .trim()
            .parse::<i32>()
            .unwrap_or(3);
        let commit_model_text = Self::value(&self.commit_model, cx);
        let (notes_path, git_pattern, solution_pattern, platform) = (
            Self::value(&self.notes_store_path, cx),
            Self::value(&self.git_folder_pattern, cx),
            Self::value(&self.solution_file_pattern, cx),
            Self::value(&self.platform_folder_name, cx),
        );
        let (vscode_exe, vscode_profile, terminal_exe) = (
            Self::value(&self.vscode_executable, cx),
            Self::value(&self.vscode_profile, cx),
            Self::value(&self.terminal_executable, cx),
        );
        let (ide_exe, opencode_exe) = (
            Self::value(&self.ide_executable, cx),
            Self::value(&self.opencode_executable, cx),
        );
        let (default_model, github_exe) = (
            Self::value(&self.default_model, cx),
            Self::value(&self.github_executable, cx),
        );
        let (azure_pat, azure_url) = (Self::value(&self.azure_pat, cx), Self::value(&self.azure_url, cx));
        let flags = (
            self.show_tooltips,
            self.start_minimized,
            self.start_at_boot,
            self.enable_terminal,
            self.enable_vscode,
            self.enable_visual_studio,
            self.enable_opencode,
            self.enable_github,
            self.enable_azure,
        );
        let snap_it_auto_start = self.snap_it_auto_start;
        let snap_it_exe = Self::value(&self.snap_it_executable, cx);
        let keep_awake_enabled = self.keep_awake_enabled;
        let warp_enabled = self.warp_enabled;
        let warp_exe = Self::value(&self.warp_executable, cx);
        let warp_notes_enabled = self.warp_notes_enabled;
        let warp_notes_exe = Self::value(&self.warp_notes_executable, cx);
        let branch_prefix = Self::value(&self.branch_name_prefix, cx);
        let auto_fetch_minutes = self.auto_fetch_minutes;
        // absence = fetch always; the only meaningful write is the opt-in
        let auto_fetch_only_focused = self.auto_fetch_only_focused;
        let group_by_root = self.group_by_root;
        let group_depth = self.group_depth;
        let code_search_enabled = self.enable_code_search;
        let code_search_exe = Self::value(&self.code_search_executable, cx);

        let old_scan_folders = self.store.read(cx).settings().repos.repo_scan_folders.clone();
        let old_font_scale = self.store.read(cx).settings().general.font_scale.clone();

        self.store.update(cx, |store, cx| {
            store.update_settings(|s| {
                s.general.notes_store_path = notes_path;
                s.general.show_tooltips = flags.0;
                s.general.start_minimized = flags.1;
                s.general.start_at_boot = flags.2;
                s.general.font_scale = Some(self.text_size.to_string());
                s.repos.repo_scan_folders = if scan_folders.is_empty() { None } else { Some(scan_folders.clone()) };
                s.repos.excluded_folders = if excluded.is_empty() { None } else { Some(excluded.clone()) };
                s.repos.git_folder_pattern = git_pattern;
                s.repos.solution_file_pattern = solution_pattern;
                s.repos.platform_folder_name = platform;
                s.repos.max_scan_depth = max_scan_depth;
                s.repos.branch_name_prefix = branch_prefix;
                s.repos.auto_fetch_minutes = auto_fetch_minutes;
                s.repos.auto_fetch_only_focused = auto_fetch_only_focused.then_some(true);
                s.repos.group_repos_by_root = Some(group_by_root);
                s.repos.group_root_depth = Some(group_depth as u8);
                // enabled stays absent: absence IS the enabled state, so the
                // only meaningful write is the opt-out
                s.repos.enable_code_search = (!code_search_enabled).then_some(false);
                s.repos.code_search_executable = (!code_search_exe.trim().is_empty()).then_some(code_search_exe);
                s.repos.vscode_executable = vscode_exe;
                s.repos.vscode_profile = vscode_profile;
                s.repos.terminal_executable = terminal_exe;
                s.repos.ide_executable = ide_exe;
                s.repos.opencode_executable = opencode_exe;
                s.repos.enable_terminal = flags.3;
                s.repos.enable_vscode = flags.4;
                s.repos.enable_visual_studio = flags.5;
                s.opencode.enable_opencode = flags.6;
                s.warp_agent.enable_warp_agent = warp_enabled;
                s.warp_agent.executable = warp_exe;
                s.warp_notes.enable_warp_notes = warp_notes_enabled;
                s.warp_notes.executable = warp_notes_exe;
                s.opencode.default_model = default_model;
                s.opencode.commit_model = (!commit_model_text.trim().is_empty()).then_some(commit_model_text);
                s.repos.enable_git_hub = flags.7;
                s.repos.git_hub_executable = github_exe;
                s.repos.enable_azure_dev_ops = flags.8;
                s.repos.azure_dev_ops_pat = azure_pat;
                s.repos.azure_dev_ops_url = azure_url;
                s.snap_it.auto_start = snap_it_auto_start;
                s.snap_it.executable = snap_it_exe;
                s.keep_awake.enabled = keep_awake_enabled;
            });
            // the interval may have changed: re-arm the periodic fetch
            store.refresh_auto_fetch_schedule();
            // grouping only reshapes the table: re-project, never rescan —
            // the scan roots themselves did not change
            store.set_group_by_root(group_by_root);
            store.set_group_depth(group_depth);
            // a scan-input change live-applies with a rescan; a text-size
            // change re-runs the kit theme (which refreshes every window, so
            // tokens and the kit rem base resize together); every save
            // re-probes activity so column toggles take effect immediately
            if store.settings().repos.repo_scan_folders != old_scan_folders {
                let settings = store.settings();
                store.kick_scan(&settings);
            }
            if store.settings().general.font_scale != old_font_scale {
                theme::set_font_scale(store.settings().general.font_scale.as_deref());
                theme::apply_kit_theme(cx);
            }
            let folders: Vec<std::path::PathBuf> = store
                .repos
                .iter()
                .filter(|repo| !repo.is_archived())
                .map(|r| r.folder_path.clone())
                .collect();
            store.kick_github_pass(&folders);
            store.kick_azure_pass(&folders);
            store.load_models(cx);
            cx.notify();
        });
        // the boot registration follows the saved flag
        devtools_core::autostart::reconcile(self.start_at_boot);
        // so does the execution state; a failure leaves the chip as the truth
        if let Err(err) = devtools_core::keepawake::set_enabled(self.keep_awake_enabled) {
            self.store.update(cx, |s, cx| {
                s.notify_error(err);
                cx.notify();
            });
        }
        self.saved_note = Some(std::time::Instant::now());
        cx.notify();
    }
}

impl Render for SettingsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        // the settled TemplateLoaded seeds the open editor here (set_value
        // needs a Window, which the store observer does not carry)
        if self.editing_template.is_some()
            && !self.template_editor_seeded
            && let Some((id, content, _)) = self.store.read(cx).template_loaded.clone()
            && self.editing_template == Some(id.as_str())
        {
            self.template_editor_seeded = true;
            self.template_editor
                .update(cx, |state, cx| state.set_value(content.as_str(), window, cx));
        }
        let just_saved = self
            .saved_note
            .map(|at| at.elapsed() < std::time::Duration::from_secs(2))
            .unwrap_or(false);
        // Side-by-side cards stop working before the window gets truly narrow:
        // each card halves, then each field halves again, and the switch
        // labels run into their neighbours. Below this the cards stack.
        let stack = window.viewport_size().width < px(1180.) * (theme::font_small() / 14.0);

        div()
            .id("settings-scroll")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .items_center()
            .p_4()
            .child(
                div()
                    .w_full()
                    .max_w(px(1240.))
                    .flex()
                    .flex_col()
                    .gap_3()
                    // identity bar: back link, title, folder jump, Save
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .id("settings-back")
                                    .a11y_button("Back to repositories")
                                    .cursor_pointer()
                                    .rounded_md()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .px_1()
                                    .py_1()
                                    .ml(px(-4.))
                                    .hover(|s| s.bg(theme::HOVER))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.back_requested = true;
                                        cx.notify();
                                    }))
                                    .child(crate::ui::icon(
                                        "icons/icon-arrow-left.svg",
                                        px(theme::icon_medium()),
                                        theme::TEXT,
                                    ))
                                    .child(
                                        div()
                                            .text_size(px(theme::font_title() + 2.))
                                            .font_weight(FontWeight::BOLD)
                                            .child("Settings"),
                                    ),
                            )
                            .child(div().flex_1())
                            .when(just_saved, |row| {
                                row.child(
                                    div()
                                        .text_size(px(theme::font_small()))
                                        .text_color(theme::SUCCESS_BRIGHT)
                                        .child("saved ✓"),
                                )
                            })
                            .child(
                                div()
                                    .id("settings-open-folder")
                                    .a11y_button("Open data folder")
                                    .cursor_pointer()
                                    .rounded_md()
                                    .px_2()
                                    .py_1p5()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1p5()
                                    .text_size(px(theme::font_small()))
                                    .text_color(theme::TEXT_MUTED)
                                    .hover(|s| s.bg(theme::HOVER).text_color(theme::TEXT))
                                    .on_click(|_, _, _| {
                                        let folder = devtools_core::paths::AppPaths::user_data_root();
                                        let _ = std::fs::create_dir_all(&folder);
                                        let _ = devtools_core::terminal::open_folder(&folder.to_string_lossy());
                                    })
                                    .child(crate::ui::icon(
                                        "icons/icon-folder.svg",
                                        px(theme::icon_small()),
                                        theme::TEXT_MUTED,
                                    ))
                                    .child("Data folder"),
                            )
                            .child(save_button(cx)),
                    )
                    // two balanced columns of cards; stacked when narrow
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .when(stack, |d| d.flex_col().items_stretch())
                            .when(!stack, |d| d.flex_row().items_start())
                            .child(
                                div()
                                    .when(!stack, |d| d.flex_1().min_w(px(0.)))
                                    .flex()
                                    .flex_col()
                                    .gap_3()
                                    .child(self.card("General", cx, |this, cx, card| {
                                        card.child(switch_row("Start at boot", this.start_at_boot, cx.listener(|this, _: &bool, _, cx| {
                                            this.start_at_boot = !this.start_at_boot;
                                            cx.notify();
                                        })))
                                        .child(switch_row("Start minimized", this.start_minimized, cx.listener(|this, _, _, cx| {
                                            this.start_minimized = !this.start_minimized;
                                            cx.notify();
                                        })))
                                        .child(switch_row("Show tooltips", this.show_tooltips, cx.listener(|this, _: &bool, _, cx| {
                                            this.show_tooltips = !this.show_tooltips;
                                            cx.notify();
                                        })))
                                        .child(field_row("Notes store path", &this.notes_store_path))
                                        .child(text_size_row(this, cx))
                                        .child(label("Applied across the app as soon as it's saved."))
                                    }))
                                    .child(self.card("Repository scanning", cx, |this, cx, card| {
                                        card.child(label("Scan folders (one per line)"))
                                            .child(Textarea::new(&this.scan_folders).bordered(true).small().h(px(240.)).aria_label("Scan folders"))
                                            .child(label("Excluded folders (one per line)"))
                                            .child(Textarea::new(&this.excluded_folders).bordered(true).small().h(px(100.)).aria_label("Excluded folders"))
                                            .child(two_col(
                                                field_row("Git folder pattern", &this.git_folder_pattern),
                                                field_row("Solution file pattern", &this.solution_file_pattern),
                                            ))
                                            .child(two_col(
                                                field_row("Platform folder name", &this.platform_folder_name),
                                                field_row("Max scan depth", &this.max_scan_depth),
                                            ))
                                            .child(switch_row("Group repos by scan root", this.group_by_root, cx.listener(|this, _, _, cx| {
                                                this.group_by_root = !this.group_by_root;
                                                cx.notify();
                                            })))
                                            .child(label("Group the table under the scan folders' shared folder — collapsing and filtering here never rescans."))
                                            .when(this.group_by_root, |card| {
                                                card.child(group_depth_row(this, cx))
                                                    .child(label("Headers sit this many folder levels under the scan folders' shared folder; deeper repos fold into their ancestor's group. Scan folders sharing no folder group individually."))
                                            })
                                    }))
                                    .child(self.card("Git", cx, |this, cx, card| {
                                        card.child(field_row("Branch name prefix", &this.branch_name_prefix))
                                            .child(auto_fetch_row(this, cx))
                                            .child(switch_row("Only fetch while active", this.auto_fetch_only_focused, cx.listener(|this, _, _, cx| {
                                                this.auto_fetch_only_focused = !this.auto_fetch_only_focused;
                                                cx.notify();
                                            })))
                                            .child(label("With an interval set, pauses the fetch while no window has focus; with Auto fetch off, fetches once on launch."))
                                    })),
                            )
                            .child(
                                div()
                                    .when(!stack, |d| d.flex_1().min_w(px(0.)))
                                    .flex()
                                    .flex_col()
                                    .gap_3()
                                    .child(self.card("External tools", cx, |this, cx, card| {
                                        card.child(two_col(
                                            switch_row("Enable Visual Studio (Windows)", this.enable_visual_studio, cx.listener(|this, _, _, cx| {
                                                this.enable_visual_studio = !this.enable_visual_studio;
                                                cx.notify();
                                            })),
                                            switch_row("Enable VS Code", this.enable_vscode, cx.listener(|this, _, _, cx| {
                                                this.enable_vscode = !this.enable_vscode;
                                                cx.notify();
                                            })),
                                        ))
                                        .child(switch_row("Enable Terminal", this.enable_terminal, cx.listener(|this, _, _, cx| {
                                            this.enable_terminal = !this.enable_terminal;
                                            cx.notify();
                                        })))
                                        .child(two_col(
                                            field_row("VS Code executable", &this.vscode_executable),
                                            field_row("VS Code profile", &this.vscode_profile),
                                        ))
                                        .child(two_col(
                                            field_row("Terminal executable", &this.terminal_executable),
                                            field_row("IDE executable", &this.ide_executable),
                                        ))
                                        .child(switch_row("Cross-repo code search", this.enable_code_search, cx.listener(|this, _, _, cx| {
                                            this.enable_code_search = !this.enable_code_search;
                                            cx.notify();
                                        })))
                                        .child(field_row("ripgrep executable", &this.code_search_executable))
                                        .when(cfg!(windows), |card| {
                                            card.child(two_col(
                                                switch_row("Start SnapIt automatically (Windows)", this.snap_it_auto_start, cx.listener(|this, _, _, cx| {
                                                    this.snap_it_auto_start = !this.snap_it_auto_start;
                                                    cx.notify();
                                                })),
                                                snapit_run_control(cx),
                                            ))
                                            .child(field_row("SnapIt executable", &this.snap_it_executable))
                                            .child(switch_row("Keep system and display awake (Windows)", this.keep_awake_enabled, cx.listener(|this, _, _, cx| {
                                                this.keep_awake_enabled = !this.keep_awake_enabled;
                                                cx.notify();
                                            })))
                                        })
                                    }))
                                    .child(self.card("OpenCode", cx, |this, cx, card| {
                                        card.child(switch_row("Enable OpenCode", this.enable_opencode, cx.listener(|this, _, _, cx| {
                                            this.enable_opencode = !this.enable_opencode;
                                            cx.notify();
                                        })))
                                        .child(field_row("OpenCode executable", &this.opencode_executable))
                                        .child(label("Shows the repo row's OpenCode button: it launches this CLI in a terminal with the default model below. Independent of warp-agent."))
                                    }))
                                    .child(self.card("AI features (warp-agent)", cx, |this, cx, card| {
                                        card.child(switch_row("Enable warp-agent", this.warp_enabled, cx.listener(|this, _, _, cx| {
                                            this.warp_enabled = !this.warp_enabled;
                                            cx.notify();
                                        })))
                                        .child(field_row("warp-agent executable", &this.warp_executable))
                                        .child(label("Default model"))
                                        .child(this.default_model_picker.clone())
                                        .child(label("Commit model (wand)"))
                                        .child(this.commit_model_picker.clone())
                                        .child(label("Prompt templates"))
                                        .child(this.templates_section(cx))
                                        .child(label("The wands, the daily summary and the model catalog run over warp-agent's door."))
                                    }))
                                    .child(self.card("Notes (warp-notes)", cx, |this, cx, card| {
                                        card.child(switch_row("Enable warp-notes", this.warp_notes_enabled, cx.listener(|this, _, _, cx| {
                                            this.warp_notes_enabled = !this.warp_notes_enabled;
                                            cx.notify();
                                        })))
                                        .child(field_row("warp-notes executable", &this.warp_notes_executable))
                                        .child(label("The notes toggle, the tools menu and the rows' note icons launch this app over the shared notes store set above."))
                                    }))
                                    .child(self.card("Columns", cx, |this, cx, card| {
                                        card.child(switch_row("GitHub column", this.enable_github, cx.listener(|this, _, _, cx| {
                                            this.enable_github = !this.enable_github;
                                            cx.notify();
                                        })))
                                        .child(field_row("GitHub CLI executable", &this.github_executable))
                                        .child(switch_row("Azure DevOps column", this.enable_azure, cx.listener(|this, _, _, cx| {
                                            this.enable_azure = !this.enable_azure;
                                            cx.notify();
                                        })))
                                        .child(field_row("Azure DevOps PAT", &this.azure_pat))
                                        .child(field_row("Azure DevOps server URL", &this.azure_url))
                                    })),
                            ),
                    ),
            )
    }
}

impl SettingsPage {
    // ---- prompt templates -------------------------------------------------

    /// The card's template manager: one row per registry entry, or the
    /// embedded editor while a template is open for editing.
    fn templates_section(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        match self.editing_template {
            Some(id) => self.template_editor_view(id, cx),
            None => self.template_rows(cx),
        }
    }

    /// One row per registry entry: title, the customized badge (stored body
    /// differs from the built-in default), the placeholder chips, and the
    /// Edit/Open/Reset actions.
    fn template_rows(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let mut list = div().flex().flex_col().gap_2();
        for template in devtools_core::opencode::PROMPT_TEMPLATES {
            let id = template.id;
            // read from disk each draw so externally-edited bodies badge on
            // the next render; every TemplateIo settle re-notifies the store
            let customized = devtools_core::opencode::is_customized(id);
            let armed = self.reset_armed == Some(id);
            let buttons = self.template_buttons(id, armed, cx);
            list = list.child(
                div()
                    .id(SharedString::from(format!("template-row-{id}")))
                    .debug_selector(move || format!("template-row-{id}"))
                    .aria_label(template.title)
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(theme::font_small()))
                                    .text_color(theme::TEXT)
                                    .child(template.title),
                            )
                            .when(customized, |row| {
                                row.child(
                                    div()
                                        .id(SharedString::from(format!(
                                            "template-customized-{id}"
                                        )))
                                        .debug_selector(move || format!("template-customized-{id}"))
                                        .rounded_full()
                                        .bg(theme::tint(theme::ACCENT))
                                        .text_color(theme::ACCENT)
                                        .text_size(px(theme::font_small() - 2.))
                                        .px_1p5()
                                        .child("customized"),
                                )
                            })
                            .child(div().flex_1())
                            .child(buttons),
                    )
                    .child(placeholder_chips(template.placeholders)),
            );
        }
        list.into_any_element()
    }

    /// Edit swaps the card into the embedded editor, Open hands the file to
    /// the system editor, and Reset is two-click armed (arming another row
    /// replaces the arm).
    fn template_buttons(
        &mut self,
        id: &'static str,
        armed: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let this = cx.entity();
        let reset_store = self.store.clone();
        div()
            .flex()
            .flex_row()
            .gap_1()
            .child(template_action_button(
                format!("template-edit-{id}"),
                "Edit",
                false,
                cx.listener(move |this, _, window, cx| {
                    this.editing_template = Some(id);
                    this.template_editor_seeded = false;
                    this.reset_armed = None;
                    // blank the buffer so the previously-edited template
                    // never flashes while the Read round-trip is in flight
                    this.template_editor
                        .update(cx, |state, cx| state.set_value("", window, cx));
                    this.store.update(cx, |s, _| s.template_io(id, TemplateOp::Read));
                    cx.notify();
                }),
            ))
            .child(template_action_button(
                format!("template-open-{id}"),
                "Open",
                false,
                move |_, _, _| {
                    // never hand the system editor a file that does not exist
                    if let Some(template) =
                        devtools_core::opencode::PROMPT_TEMPLATES.iter().find(|t| t.id == id)
                        && devtools_core::opencode::read_template(id).is_none()
                    {
                        let _ = devtools_core::opencode::write_template(id, template.default);
                    }
                    if let Some(path) = devtools_core::opencode::template_path(id) {
                        let _ =
                            devtools_core::terminal::open_folder(&path.to_string_lossy());
                    }
                },
            ))
            .child(template_action_button(
                format!("template-reset-{id}"),
                "Reset",
                armed,
                move |_, _, cx| {
                    if armed {
                        reset_store.update(cx, |s, _| s.template_io(id, TemplateOp::Reset));
                        this.update(cx, |this, cx| {
                            this.reset_armed = None;
                            cx.notify();
                        });
                    } else {
                        this.update(cx, |this, cx| {
                            this.reset_armed = Some(id);
                            cx.notify();
                        });
                    }
                },
            ))
            .into_any_element()
    }

    /// The embedded editor for one template. The body arrives via the store
    /// (TemplateLoaded) and render seeds the buffer, so the editor shows a
    /// loading label until the event lands.
    fn template_editor_view(
        &mut self,
        id: &'static str,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let template = devtools_core::opencode::PROMPT_TEMPLATES
            .iter()
            .find(|t| t.id == id)
            .expect("editing_template ids come from the registry");
        let title = template.title;
        let editor = Editor::new(&self.template_editor)
            .bordered(false)
            .appearance(false)
            .h_full()
            .aria_label(format!("{title} template body"))
            .font_family(SharedString::from(theme::ui_font()))
            .text_size(px(theme::font_body() + 1.))
            .line_height(relative(1.7));
        div()
            .id(SharedString::from(format!("template-editor-{id}")))
            .debug_selector(move || format!("template-editor-{id}"))
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(theme::font_small()))
                            .text_color(theme::TEXT)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(theme::font_small() - 1.))
                            .text_color(theme::TEXT_FAINT)
                            .child(if self.template_editor_seeded {
                                "re-read by every wand run"
                            } else {
                                "loading…"
                            }),
                    )
                    .child(div().flex_1())
                    .child(template_action_button(
                        format!("template-save-{id}"),
                        "Save",
                        false,
                        cx.listener(move |this, _, _, cx| {
                            let content = this.template_editor.read(cx).value().to_string();
                            this.store
                                .update(cx, |s, _| s.template_io(id, TemplateOp::Write(content)));
                            this.store.update(cx, |s, cx| {
                                s.notify_success(format!("{title} template saved"));
                                cx.notify();
                            });
                            this.editing_template = None;
                            this.template_editor_seeded = false;
                            cx.notify();
                        }),
                    ))
                    .child(template_action_button(
                        format!("template-cancel-{id}"),
                        "Cancel",
                        false,
                        cx.listener(move |this, _, _, cx| {
                            this.editing_template = None;
                            this.template_editor_seeded = false;
                            cx.notify();
                        }),
                    )),
            )
            .child(
                div()
                    .h(px(320.))
                    .rounded_md()
                    .border_1()
                    .border_color(theme::HAIRLINE)
                    .p_2()
                    .child(editor),
            )
            .child(placeholder_chips(template.placeholders))
            .into_any_element()
    }

    fn card(
        &mut self,
        title: &str,
        cx: &mut Context<Self>,
        build: impl FnOnce(&mut Self, &mut Context<Self>, gpui::Div) -> gpui::Div,
    ) -> gpui::AnyElement {
        let shell = div()
            .w_full()
            .rounded_xl()
            .border_1()
            .border_color(theme::HAIRLINE)
            .bg(theme::CARD)
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_size(px(theme::font_title()))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title.to_string()),
            );
        build(self, cx, shell).into_any_element()
    }
}

fn save_button(cx: &mut Context<SettingsPage>) -> gpui::AnyElement {
    div()
        .id("settings-save")
        .debug_selector(|| "settings-save".into())
        .a11y_button("Save settings")
        .cursor_pointer()
        .rounded_md()
        .bg(theme::ACCENT)
        .px_4()
        .py_1p5()
        .text_size(px(theme::font_small()))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::ON_ACCENT)
        .hover(|s| s.opacity(0.9))
        .on_click(cx.listener(|this, _, _, cx| this.save(cx)))
        .child("Save")
        .into_any_element()
}

fn label(text: &str) -> impl IntoElement + use<> {
    div()
        .text_size(px(theme::font_small()))
        .text_color(theme::TEXT_MUTED)
        .child(text.to_string())
}

/// A small bordered row action; the armed state turns red with "Confirm".
fn template_action_button(
    id: String,
    label: &str,
    armed: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> gpui::AnyElement {
    let text = if armed { "Confirm" } else { label };
    let selector = id.clone();
    div()
        .id(SharedString::from(id))
        .debug_selector(move || selector)
        .a11y_button(text)
        .cursor_pointer()
        .rounded_md()
        .border_1()
        .border_color(if armed { theme::DANGER } else { theme::HAIRLINE })
        .px_2()
        .py_0p5()
        .text_size(px(theme::font_small() - 1.))
        .text_color(if armed { theme::DANGER } else { theme::TEXT_DIM })
        .hover(|s| s.bg(theme::HOVER))
        .on_click(on_click)
        .child(text.to_string())
        .into_any_element()
}

/// The template's fill-in placeholders as tiny mono chips.
fn placeholder_chips(placeholders: &[&'static str]) -> impl IntoElement + use<> {
    let mut row = div().flex().flex_row().flex_wrap().gap_1();
    for placeholder in placeholders {
        row = row.child(
            div()
                .rounded_md()
                .bg(theme::CHIP)
                .px_1p5()
                .py_0p5()
                .text_size(px(theme::font_small() - 2.))
                .text_color(theme::TEXT_MUTED)
                .font_family(SharedString::from(theme::code_font()))
                .child(placeholder.to_string()),
        );
    }
    row
}

/// Two fields (or switches) sharing one row, each half of the card width.
fn two_col(a: impl IntoElement, b: impl IntoElement) -> gpui::AnyElement {
    div()
        .flex()
        .flex_row()
        .gap_3()
        .child(div().flex_1().min_w(px(0.)).child(a))
        .child(div().flex_1().min_w(px(0.)).child(b))
        .into_any_element()
}

fn field_row(label_text: &str, field: &Entity<InputState>) -> impl IntoElement + use<> {
    // a stable per-row selector so probes can reach the field through its row
    let selector = format!("settings-field-{label_text}");
    div()
        .debug_selector(move || selector.clone())
        .flex()
        .flex_col()
        .gap_1()
        .child(label(label_text))
        .child(Input::new(field).small().aria_label(label_text))
}

fn snapit_run_control(cx: &mut Context<SettingsPage>) -> gpui::AnyElement {
    let running = devtools_core::snapit::is_running();
    div()
        .id("snapit-run-control")
        .a11y_button(if running { "Stop SnapIt" } else { "Start SnapIt" })
        .cursor_pointer()
        .self_center()
        .rounded_md()
        .border_1()
        .border_color(theme::HAIRLINE)
        .px_3()
        .py_1p5()
        .text_size(px(theme::font_small()))
        .text_color(if running { theme::DANGER } else { theme::SUCCESS_BRIGHT })
        .hover(|s| s.bg(theme::HOVER))
        .on_click(cx.listener(|this, _, _, cx| {
            // the live field value, so a new binary can be tried without
            // saving the page first
            let exe = SettingsPage::value(&this.snap_it_executable, cx);
            let result = if devtools_core::snapit::is_running() {
                devtools_core::snapit::stop()
            } else {
                devtools_core::snapit::start(&exe)
            };
            if let Err(err) = result {
                this.store.update(cx, |s, cx| {
                    s.notify_error(err);
                    cx.notify();
                });
            }
            cx.notify();
        }))
        .child(if running { "Stop SnapIt" } else { "Start SnapIt" })
        .into_any_element()
}

fn switch_row(
    label_text: &str,
    checked: bool,
    on_click: impl Fn(&bool, &mut Window, &mut gpui::App) + 'static,
) -> gpui::AnyElement {
    // a stable per-row selector so probes can reach the switch through its row
    let selector = format!("settings-row-{label_text}");
    div()
        .debug_selector(move || selector.clone())
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(
            Switch::new(SharedString::from(format!("switch-{label_text}")))
                .checked(checked)
                .accessibility_label(label_text)
                .on_click(on_click),
        )
        // the label gives up room (and wraps) instead of running under the
        // neighbouring column when the card is narrow
        .child(div().flex_1().min_w_0().child(label(label_text)))
        .into_any_element()
}

fn text_size_row(this: &SettingsPage, cx: &mut Context<SettingsPage>) -> gpui::AnyElement {
    let options = [(theme::FONT_COMPACT, "Small"), (theme::FONT_NORMAL, "Normal"), (theme::FONT_LARGE, "Large")];
    let mut row = div().flex().flex_row().items_center().gap_2().child(label("Text size"));
    for (key, name) in options {
        let selected = this.text_size == key;
        row = row.child(
            div()
                .id(SharedString::from(format!("text-size-{key}")))
                .a11y_toggle(format!("Text size {name}"), selected)
                .cursor_pointer()
                .rounded_md()
                .px_3()
                .py_1()
                .text_size(px(theme::font_small()))
                .font_weight(FontWeight::SEMIBOLD)
                .when(selected, |d| d.bg(theme::tint(theme::ACCENT)).text_color(theme::ACCENT))
                .when(!selected, |d| d.text_color(theme::TEXT_MUTED).hover(|s| s.bg(theme::HOVER)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.text_size = key;
                    cx.notify();
                }))
                .child(name),
        );
    }
    row.into_any_element()
}

/// The group-depth picker: 1/2/3/4 segmented buttons; 1 = the scan roots'
/// shared folder. Only rendered while grouping is on.
fn group_depth_row(this: &SettingsPage, cx: &mut Context<SettingsPage>) -> gpui::AnyElement {
    let mut row = div()
        .id("group-depth-row")
        .debug_selector(|| "group-depth-row".into())
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(label("Group depth"));
    for level in 1..=4usize {
        let id = SharedString::from(format!("group-depth-{level}"));
        let selected = this.group_depth == level;
        row = row.child(
            div()
                .id(id.clone())
                .debug_selector(move || id.to_string())
                .a11y_toggle(format!("Group depth {level}"), selected)
                .cursor_pointer()
                .rounded_md()
                .px_3()
                .py_1()
                .text_size(px(theme::font_small()))
                .font_weight(FontWeight::SEMIBOLD)
                .when(selected, |d| d.bg(theme::tint(theme::ACCENT)).text_color(theme::ACCENT))
                .when(!selected, |d| d.text_color(theme::TEXT_MUTED).hover(|s| s.bg(theme::HOVER)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.group_depth = level;
                    cx.notify();
                }))
                .child(level.to_string()),
        );
    }
    row.into_any_element()
}

/// The periodic-fetch picker: Off/15m/30m/60m segmented buttons; None = off.
fn auto_fetch_row(this: &SettingsPage, cx: &mut Context<SettingsPage>) -> gpui::AnyElement {
    let options: [(Option<u16>, &str, &'static str); 4] = [
        (None, "Off", "auto-fetch-off"),
        (Some(15), "15m", "auto-fetch-15"),
        (Some(30), "30m", "auto-fetch-30"),
        (Some(60), "60m", "auto-fetch-60"),
    ];
    let mut row = div()
        .id("auto-fetch-row")
        .debug_selector(|| "auto-fetch-row".into())
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(label("Auto fetch"));
    for (value, name, id) in options {
        let selected = this.auto_fetch_minutes == value;
        row = row.child(
            div()
                .id(id)
                .debug_selector(move || id.into())
                .a11y_toggle(format!("Auto fetch {name}"), selected)
                .cursor_pointer()
                .rounded_md()
                .px_3()
                .py_1()
                .text_size(px(theme::font_small()))
                .font_weight(FontWeight::SEMIBOLD)
                .when(selected, |d| d.bg(theme::tint(theme::ACCENT)).text_color(theme::ACCENT))
                .when(!selected, |d| d.text_color(theme::TEXT_MUTED).hover(|s| s.bg(theme::HOVER)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.auto_fetch_minutes = value;
                    cx.notify();
                }))
                .child(name),
        );
    }
    row.into_any_element()
}

// ---- model pickers ----------------------------------------------------------

/// The app's icon paths for the shared picker shell.
const PICKER_ICONS: PickerIcons = PickerIcons {
    check: "icons/icon-check.svg",
    chevron_down: "icons/icon-chevron-down.svg",
    trash: "icons/icon-trash.svg",
    refresh: "icons/icon-refresh.svg",
};

/// One settings model slot: a shared FilteredPicker whose pick writes the
/// model into `field`; the catalog, grouping and refresh come from the store.
fn model_picker(
    field: &Entity<InputState>,
    store: &Entity<Store>,
    key: &'static str,
    default_label: &'static str,
    window: &mut Window,
    cx: &mut Context<SettingsPage>,
) -> Entity<FilteredPicker> {
    let spec = PickerSpec {
        key,
        trigger_label: key,
        filter_placeholder: "Filter models",
        icons: PICKER_ICONS,
        trigger_icon: None,
        provider_badge: true,
        refresh: true,
        refresh_label: "Refresh models",
        stop_trigger_mousedown: false,
        visible_rows: 10,
        selected_accent: false,
        overlay: false,
        anchor: OverlayAnchor::Rail,
        trigger_scale: TriggerScale::Compact,
        trigger_rounded_lg: false,
        trigger_min_h: None,
        panel_chrome: PanelChrome::Menus,
        trigger_id: None,
        panel_id: None,
        filter_selector: None,
        list_id: None,
    };
    let refresh_store = store.clone();
    cx.new(|cx| {
        let data = {
            let field = field.clone();
            let store = store.clone();
            Box::new(move |filter: &str, window: &gpui::Window, cx: &gpui::App| {
                model_data(&field, &store, key, default_label, filter, window, cx)
            }) as Box<dyn Fn(&str, &gpui::Window, &gpui::App) -> PickerData>
        };
        let mut picker =
            FilteredPicker::new(spec, PickerTarget::Field(field.clone()), data, window, cx);
        picker.set_on_refresh(Box::new(move |cx| {
            refresh_store.update(cx, |s, cx| s.load_models(cx));
        }));
        picker.observe_entity(store, cx);
        picker.observe_entity(field, cx);
        picker
    })
}

/// One render's worth of the model catalog: the default row, provider groups
/// and the filtered, deduped models.
fn model_data(
    field: &Entity<InputState>,
    store: &Entity<Store>,
    key: &'static str,
    default_label: &str,
    filter: &str,
    _window: &gpui::Window,
    cx: &gpui::App,
) -> PickerData {
    let selected = field.read(cx).value().trim().to_string();
    let filter = filter.to_lowercase();
    let mut models: Vec<String> = store
        .read(cx)
        .models
        .iter()
        .filter(|model| {
            let model = model.trim();
            !model.is_empty()
                && !model.starts_with('(')
                && (filter.is_empty() || model.to_lowercase().contains(&filter))
        })
        .cloned()
        .collect();
    models.sort_by(|a, b| {
        model_provider(a)
            .to_lowercase()
            .cmp(&model_provider(b).to_lowercase())
            .then_with(|| a.to_lowercase().cmp(&b.to_lowercase()))
    });
    models.dedup_by(|a, b| a.eq_ignore_ascii_case(b));

    let mut rows = Vec::with_capacity(models.len() + 4);
    rows.push(PickerRow::Item(PickerItem {
        id: format!("model-{key}-default"),
        selector: format!("model-row-{key}-default"),
        value: String::new(),
        label: default_label.to_string(),
        selected: selected.is_empty(),
        muted: false,
        trailing_delete: None,
    }));
    if models.is_empty() {
        rows.push(PickerRow::Message {
            id: format!("model-message-{key}"),
            label: "No models found. Start warp-agent, then refresh.".to_string(),
        });
    } else {
        let mut last_provider = String::new();
        for model in models {
            let provider = model_provider(&model).to_string();
            if !provider.is_empty() && provider != last_provider {
                rows.push(PickerRow::Group {
                    id: format!("model-provider-{key}-{provider}"),
                    label: provider.to_uppercase(),
                    selector: None,
                });
                last_provider = provider;
            }
            rows.push(PickerRow::Item(PickerItem {
                id: format!("model-row-{key}-{model}"),
                selector: format!("model-row-{key}-{model}"),
                value: model.clone(),
                label: model_name(&model).to_string(),
                selected: model.eq_ignore_ascii_case(&selected),
                muted: false,
                trailing_delete: None,
            }));
        }
    }

    PickerData {
        body: PickerBody::Rows(rows),
        selected: Selection {
            badge: (!model_provider(&selected).is_empty()).then(|| model_provider(&selected).to_string()),
            label: if selected.is_empty() {
                default_label.to_string()
            } else {
                model_name(&selected).to_string()
            },
            value: selected,
        },
        footer: None,
        fonts: theme::font_tokens(),
        accent: theme::ACCENT,
        show_tooltips: store.read(cx).settings().general.show_tooltips,
    }
}
