//! The Settings page: every settings section as a card of labeled inputs and
//! switches, seeded from the live settings when the page opens. Save writes
//! the whole graph atomically and live-applies (a scan-related change also
//! kicks a rescan). The model pickers list the opencode catalog with a
//! refresh button.

use crate::store::Store;
use crate::theme;
use gpui::prelude::*;
use gpui::{div, px, Context, Entity, FontWeight, SharedString, Window};
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::switch::Switch;

pub struct SettingsPage {
    pub store: Entity<Store>,
    // General
    notes_store_path: Entity<InputState>,
    show_tooltips: bool,
    start_minimized: bool,
    start_at_boot: bool,
    // Scanning
    scan_folders: Entity<TextareaState>,
    excluded_folders: Entity<TextareaState>,
    git_folder_pattern: Entity<InputState>,
    solution_file_pattern: Entity<InputState>,
    platform_folder_name: Entity<InputState>,
    max_scan_depth: Entity<InputState>,
    // Git
    branch_name_prefix: Entity<InputState>,
    // External tools
    vscode_executable: Entity<InputState>,
    vscode_profile: Entity<InputState>,
    terminal_executable: Entity<InputState>,
    ide_executable: Entity<InputState>,
    opencode_executable: Entity<InputState>,
    zcode_executable: Entity<InputState>,
    enable_terminal: bool,
    enable_vscode: bool,
    enable_visual_studio: bool,
    enable_zcode: bool,
    // OpenCode
    enable_opencode: bool,
    default_model: Entity<InputState>,
    commit_model: Entity<InputState>,
    default_model_open: bool,
    commit_model_open: bool,
    // Columns
    enable_github: bool,
    github_executable: Entity<InputState>,
    enable_azure: bool,
    azure_pat: Entity<InputState>,
    azure_url: Entity<InputState>,
    saved_note: Option<std::time::Instant>,
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

        let page = Self {
            store: store.clone(),
            notes_store_path: field(window, cx, &general.notes_store_path),
            show_tooltips: general.show_tooltips,
            start_minimized: general.start_minimized,
            start_at_boot: general.start_at_boot,
            scan_folders: area(window, cx, &line_list(&repos.repo_scan_folders)),
            excluded_folders: area(window, cx, &line_list(&repos.excluded_folders)),
            git_folder_pattern: field(window, cx, &repos.git_folder_pattern),
            solution_file_pattern: field(window, cx, &repos.solution_file_pattern),
            platform_folder_name: field(window, cx, &repos.platform_folder_name),
            max_scan_depth: field(window, cx, &repos.max_scan_depth.to_string()),
            branch_name_prefix: field(window, cx, &repos.branch_name_prefix),
            vscode_executable: field(window, cx, &repos.vscode_executable),
            vscode_profile: field(window, cx, &repos.vscode_profile),
            terminal_executable: field(window, cx, &repos.terminal_executable),
            ide_executable: field(window, cx, &repos.ide_executable),
            opencode_executable: field(window, cx, &repos.opencode_executable),
            zcode_executable: field(window, cx, &repos.zcode_executable),
            enable_terminal: repos.enable_terminal,
            enable_vscode: repos.enable_vscode,
            enable_visual_studio: repos.enable_visual_studio,
            enable_zcode: repos.enable_zcode,
            enable_opencode: settings.opencode.enable_opencode,
            default_model: field(window, cx, &settings.opencode.default_model),
            commit_model: field(window, cx, &settings.opencode.commit_model.clone().unwrap_or_default()),
            default_model_open: false,
            commit_model_open: false,
            enable_github: repos.enable_git_hub,
            github_executable: field(window, cx, &repos.git_hub_executable),
            enable_azure: repos.enable_azure_dev_ops,
            azure_pat: field(window, cx, &repos.azure_dev_ops_pat),
            azure_url: field(window, cx, &repos.azure_dev_ops_url),
            saved_note: None,
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
        let (ide_exe, opencode_exe, zcode_exe) = (
            Self::value(&self.ide_executable, cx),
            Self::value(&self.opencode_executable, cx),
            Self::value(&self.zcode_executable, cx),
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
            self.enable_zcode,
            self.enable_opencode,
            self.enable_github,
            self.enable_azure,
        );
        let branch_prefix = Self::value(&self.branch_name_prefix, cx);

        let old_scan_folders = self.store.read(cx).settings().repos.repo_scan_folders.clone();

        self.store.update(cx, |store, cx| {
            store.update_settings(|s| {
                s.general.notes_store_path = notes_path;
                s.general.show_tooltips = flags.0;
                s.general.start_minimized = flags.1;
                s.general.start_at_boot = flags.2;
                s.repos.repo_scan_folders = if scan_folders.is_empty() { None } else { Some(scan_folders.clone()) };
                s.repos.excluded_folders = if excluded.is_empty() { None } else { Some(excluded.clone()) };
                s.repos.git_folder_pattern = git_pattern;
                s.repos.solution_file_pattern = solution_pattern;
                s.repos.platform_folder_name = platform;
                s.repos.max_scan_depth = max_scan_depth;
                s.repos.branch_name_prefix = branch_prefix;
                s.repos.vscode_executable = vscode_exe;
                s.repos.vscode_profile = vscode_profile;
                s.repos.terminal_executable = terminal_exe;
                s.repos.ide_executable = ide_exe;
                s.repos.opencode_executable = opencode_exe;
                s.repos.zcode_executable = zcode_exe;
                s.repos.enable_terminal = flags.3;
                s.repos.enable_vscode = flags.4;
                s.repos.enable_visual_studio = flags.5;
                s.repos.enable_zcode = flags.6;
                s.opencode.enable_opencode = flags.7;
                s.opencode.default_model = default_model;
                s.opencode.commit_model = (!commit_model_text.trim().is_empty()).then_some(commit_model_text);
                s.repos.enable_git_hub = flags.8;
                s.repos.git_hub_executable = github_exe;
                s.repos.enable_azure_dev_ops = flags.9;
                s.repos.azure_dev_ops_pat = azure_pat;
                s.repos.azure_dev_ops_url = azure_url;
            });
            // a scan-input change live-applies with a rescan; every save
            // re-probes activity so column toggles take effect immediately
            if store.settings().repos.repo_scan_folders != old_scan_folders {
                let settings = store.settings();
                store.kick_scan(&settings);
            }
            let folders: Vec<std::path::PathBuf> = store.repos.iter().map(|r| r.folder_path.clone()).collect();
            store.kick_github_pass(&folders);
            store.kick_azure_pass(&folders);
            store.load_models(cx);
            cx.notify();
        });
        self.saved_note = Some(std::time::Instant::now());
        cx.notify();
    }
}

impl Render for SettingsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let just_saved = self
            .saved_note
            .map(|at| at.elapsed() < std::time::Duration::from_secs(2))
            .unwrap_or(false);

        div()
            .id("settings-scroll")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .items_center()
            .p_4()
            .gap_3()
            .child(
                div()
                    .w(px(720.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(theme::FONT_TITLE + 2.))
                            .font_weight(FontWeight::BOLD)
                            .child("Settings"),
                    )
                    .child(div().flex_1())
                    .when(just_saved, |row| {
                        row.child(
                            div()
                                .text_size(px(theme::FONT_SMALL))
                                .text_color(theme::SUCCESS_BRIGHT)
                                .child("saved ✓"),
                        )
                    })
                    .child(save_button(cx)),
            )
            .child(self.card("General", cx, |this, cx, card| {
                card.child(field_row("Notes store path", &this.notes_store_path))
                    .child(switch_row("Show tooltips", this.show_tooltips, cx.listener(|this, _: &bool, _, cx| {
                        this.show_tooltips = !this.show_tooltips;
                        cx.notify();
                    })))
                    .child(switch_row("Start minimized", this.start_minimized, cx.listener(|this, _, _, cx| {
                        this.start_minimized = !this.start_minimized;
                        cx.notify();
                    })))
                    .child(switch_row("Start at boot", this.start_at_boot, cx.listener(|this, _, _, cx| {
                        this.start_at_boot = !this.start_at_boot;
                        cx.notify();
                    })))
            }))
            .child(self.card("Repository scanning", cx, |this, cx, card| {
                card.child(label("Scan folders (one per line)"))
                    .child(Textarea::new(&this.scan_folders).bordered(true).small())
                    .child(label("Excluded folders (one per line)"))
                    .child(Textarea::new(&this.excluded_folders).bordered(true).small())
                    .child(field_row("Git folder pattern", &this.git_folder_pattern))
                    .child(field_row("Solution file pattern", &this.solution_file_pattern))
                    .child(field_row("Platform folder name", &this.platform_folder_name))
                    .child(field_row("Max scan depth", &this.max_scan_depth))
            }))
            .child(self.card("Git", cx, |this, _cx, card| {
                card.child(field_row("Branch name prefix", &this.branch_name_prefix))
            }))
            .child(self.card("External tools", cx, |this, cx, card| {
                card.child(field_row("VS Code executable", &this.vscode_executable))
                    .child(field_row("VS Code profile", &this.vscode_profile))
                    .child(field_row("Terminal executable", &this.terminal_executable))
                    .child(field_row("IDE executable", &this.ide_executable))
                    .child(field_row("OpenCode executable", &this.opencode_executable))
                    .child(field_row("ZCode executable", &this.zcode_executable))
                    .child(switch_row("Terminal button", this.enable_terminal, cx.listener(|this, _, _, cx| {
                        this.enable_terminal = !this.enable_terminal;
                        cx.notify();
                    })))
                    .child(switch_row("VS Code button", this.enable_vscode, cx.listener(|this, _, _, cx| {
                        this.enable_vscode = !this.enable_vscode;
                        cx.notify();
                    })))
                    .child(switch_row("Visual Studio button (Windows)", this.enable_visual_studio, cx.listener(|this, _, _, cx| {
                        this.enable_visual_studio = !this.enable_visual_studio;
                        cx.notify();
                    })))
                    .child(switch_row("ZCode button", this.enable_zcode, cx.listener(|this, _, _, cx| {
                        this.enable_zcode = !this.enable_zcode;
                        cx.notify();
                    })))
            }))
            .child(self.card("OpenCode", cx, |this, cx, card| {
                card.child(switch_row("Enable OpenCode", this.enable_opencode, cx.listener(|this, _, _, cx| {
                    this.enable_opencode = !this.enable_opencode;
                    cx.notify();
                })))
                .child(label("Default model"))
                .child({
                    let field = this.default_model.clone();
                    this.model_picker(&field, PickerSlot::Default, cx)
                })
                .child(label("Commit model (wand)"))
                .child({
                    let field = this.commit_model.clone();
                    this.model_picker(&field, PickerSlot::Commit, cx)
                })
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
            }))
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum PickerSlot {
    Default,
    Commit,
}

impl SettingsPage {
    fn model_picker(
        &mut self,
        field: &Entity<InputState>,
        slot: PickerSlot,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let open = match slot {
            PickerSlot::Default => self.default_model_open,
            PickerSlot::Commit => self.commit_model_open,
        };
        let filter = field.read(cx).value().to_string().to_lowercase();
        let models = self.store.read(cx).models.clone();

        let mut list = div().flex().flex_col().gap_0p5().max_h(px(180.)).overflow_hidden();
        let mut shown = 0;
        for model in &models {
            if !filter.is_empty() && !model.to_lowercase().contains(&filter) {
                continue;
            }
            list = list.child(
                div()
                    .id(SharedString::from(format!("model-{model}")))
                    .cursor_pointer()
                    .rounded_md()
                    .px_2()
                    .py_0p5()
                    .text_size(px(theme::FONT_SMALL))
                    .text_color(theme::TEXT_DIM)
                    .hover(|s| s.bg(theme::HOVER))
                    .on_click({
                        let model = model.clone();
                        let field = field.clone();
                        move |_, window, cx| {
                            field.update(cx, |state, cx| state.set_value(model.as_str(), window, cx));
                        }
                    })
                    .child(model.clone()),
            );
            shown += 1;
            if shown > 20 {
                break;
            }
        }

        div()
            .relative()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_1()
                    .child(div().flex_1().child(Input::new(field).small()))
                    .child(
                        div()
                            .id(SharedString::from(format!("picker-toggle-{:?}", slot)))
                            .cursor_pointer()
                            .rounded_md()
                            .px_2()
                            .py_1()
                            .text_size(px(theme::FONT_SMALL))
                            .text_color(theme::TEXT_MUTED)
                            .border_1()
                            .border_color(theme::HAIRLINE)
                            .hover(|s| s.bg(theme::HOVER))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                match slot {
                                    PickerSlot::Default => this.default_model_open = !this.default_model_open,
                                    PickerSlot::Commit => this.commit_model_open = !this.commit_model_open,
                                }
                                cx.notify();
                            }))
                            .child(if open { "▲" } else { "▼" }),
                    )
                    .child({
                        let store = self.store.clone();
                        div()
                            .id(SharedString::from(format!("picker-refresh-{:?}", slot)))
                            .cursor_pointer()
                            .rounded_md()
                            .px_2()
                            .py_1()
                            .text_size(px(theme::FONT_SMALL))
                            .text_color(theme::TEXT_MUTED)
                            .border_1()
                            .border_color(theme::HAIRLINE)
                            .hover(|s| s.bg(theme::HOVER))
                            .on_click(move |_, _, cx| {
                                store.update(cx, |s, cx| s.load_models(cx));
                            })
                            .child("⟳")
                    }),
            )
            .when(open, |picker| {
                picker.child(
                    div()
                        .id(SharedString::from(format!("picker-list-{:?}", slot)))
                        .rounded_md()
                        .border_1()
                        .border_color(theme::HAIRLINE)
                        .bg(theme::POPUP)
                        .p_1()
                        .child(list),
                )
            })
            .into_any_element()
    }

    fn card(
        &mut self,
        title: &str,
        cx: &mut Context<Self>,
        build: impl FnOnce(&mut Self, &mut Context<Self>, gpui::Div) -> gpui::Div,
    ) -> gpui::AnyElement {
        let shell = div()
            .w(px(720.))
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
                    .text_size(px(theme::FONT_TITLE))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title.to_string()),
            );
        build(self, cx, shell).into_any_element()
    }
}

fn save_button(cx: &mut Context<SettingsPage>) -> gpui::AnyElement {
    div()
        .id("settings-save")
        .cursor_pointer()
        .rounded_md()
        .px_4()
        .py_1p5()
        .text_size(px(theme::FONT_SMALL))
        .font_weight(FontWeight::SEMIBOLD)
        .bg(theme::ACCENT)
        .text_color(theme::ON_ACCENT)
        .hover(|s| s.opacity(0.9))
        .on_click(cx.listener(|this, _, _, cx| this.save(cx)))
        .child("Save")
        .into_any_element()
}

fn label(text: &str) -> impl IntoElement + use<> {
    div()
        .text_size(px(theme::FONT_SMALL))
        .text_color(theme::TEXT_MUTED)
        .child(text.to_string())
}

fn field_row(label_text: &str, field: &Entity<InputState>) -> impl IntoElement + use<> {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(label(label_text))
        .child(Input::new(field).small())
}

fn switch_row(
    label_text: &str,
    checked: bool,
    on_click: impl Fn(&bool, &mut Window, &mut gpui::App) + 'static,
) -> gpui::AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(Switch::new(SharedString::from(format!("switch-{label_text}"))).checked(checked).on_click(on_click))
        .child(label(label_text))
        .into_any_element()
}
