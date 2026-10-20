// Rust | ui/tests.rs | GPUI TestPlatform | in-process native interaction regressions
use super::{NativeView, Page, catalog};
use gpui_kit::{
    AnyWindowHandle, App, AppContext, Bounds, Context, Entity, Focusable, InputEvent as _,
    KeyDownEvent, KeyUpEvent, Keystroke, TestAppContext, Window, WindowBounds, WindowOptions,
    component::{Theme, ThemeMode},
    point, px, size,
};
use skillissue_core::models::{ConfigView, default_shortcuts};
use skillissue_platform::shortcuts::Action as ShortcutAction;
use std::{collections::BTreeSet, path::PathBuf, sync::Arc};

struct Fixture {
    window: AnyWindowHandle,
    view: Entity<NativeView>,
    unused_data_dir: PathBuf,
}

#[gpui_kit::test]
fn byok_fields_and_actions_share_control_height(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        for width in [640., 800., 1100.] {
            fixture.route(cx, "settings-provider");
            fixture.update(cx, |view, window, cx| {
                Theme::change(theme, Some(window), cx);
                view.settings_ui.provider_mode = super::ProviderMode::Byok;
                view.endpoint.update(cx, |input, cx| {
                    input.set_value("https://api.openai.com/v1", window, cx)
                });
            });
            cx.simulate_window_resize(fixture.window, size(px(width), px(800.)));
            fixture.render(cx);
            fixture.update(cx, |_, _, cx| {
                let theme = Theme::global(cx);
                let expected: gpui_kit::Hsla = gpui_kit::rgb(if theme.mode.is_dark() {
                    0x172236
                } else {
                    0xe1e8f4
                })
                .into();
                assert_eq!(
                    theme.background, expected,
                    "theme switches preserve the app palette"
                );
                for surface in [
                    theme.button,
                    theme.popover,
                    theme.group_box,
                    theme.colors.list,
                ] {
                    assert_eq!(surface.a, 1., "component surfaces must be opaque");
                }
            });
            let mut visual = gpui_kit::VisualTestContext::from_window(fixture.window, cx);
            let key = visual
                .debug_bounds("connection-key-input-0")
                .expect("Text API key");
            let test = visual
                .debug_bounds("settings-action-Test")
                .expect("Test action");
            assert_eq!(test.size.height, px(super::style::CONTROL_HEIGHT));
            assert_eq!(key.size.height, test.size.height);
            assert!(key.right() <= px(width));
            assert!(test.right() <= px(width));
        }
    }
}
#[gpui_kit::test]
fn home_actions_and_settings_tabs_remain_reachable_at_narrow_widths(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        for width in [640., 800., 1100.] {
            fixture.update(cx, |_, window, cx| Theme::change(theme, Some(window), cx));
            fixture.route(cx, "dashboard");
            cx.simulate_window_resize(fixture.window, size(px(width), px(700.)));
            fixture.render(cx);
            let mut visual = gpui_kit::VisualTestContext::from_window(fixture.window, cx);
            assert!(visual.debug_bounds("workspace-navigation").is_none());
            assert!(visual.debug_bounds("dashboard-back").is_none());
            let search = visual
                .debug_bounds("dashboard-search")
                .expect("session search");
            let menu = visual
                .debug_bounds("dashboard-menu")
                .expect("workspace menu");
            assert!(search.size.width >= px(width * 0.5));
            assert_eq!(search.size.height, menu.size.height);
            assert!(search.right() <= menu.left());
            let settings = visual
                .debug_bounds("home-settings")
                .expect("home settings action");
            assert!(settings.bottom() <= px(700.));
            assert!(settings.right() <= px(width));
            visual.simulate_click(settings.center(), Default::default());
            drop(visual);
            fixture.render(cx);
            for (label, tab) in [
                (
                    "settings-tab-AI providers",
                    super::settings::SettingsTab::Provider,
                ),
                (
                    "settings-tab-Language",
                    super::settings::SettingsTab::Language,
                ),
                (
                    "settings-tab-Keybinds",
                    super::settings::SettingsTab::Shortcuts,
                ),
                (
                    "settings-tab-General",
                    super::settings::SettingsTab::General,
                ),
            ] {
                let mut visual = gpui_kit::VisualTestContext::from_window(fixture.window, cx);
                let bounds = visual.debug_bounds(label).expect("visible settings tab");
                let content = visual
                    .debug_bounds("settings-content")
                    .expect("scrolling content");
                assert!(bounds.right() <= px(width));
                assert!(bounds.bottom() <= content.top());
                assert!(content.size.height >= px(200.));
                visual.simulate_click(bounds.center(), Default::default());
                drop(visual);
                fixture.render(cx);
                fixture.read(cx, |view, _| assert!(view.settings_ui.tab == tab));
            }
        }
    }
}
#[gpui_kit::test]
fn setup_flow_advances_and_preserves_step_after_settings(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        fixture.update(cx, |view, window, cx| {
            Theme::change(theme, Some(window), cx);
            view.dashboard.setup_step = 0;
            view.dashboard.setup_dismissed = false;
        });
        for (step, tab) in [
            super::settings::SettingsTab::Provider,
            super::settings::SettingsTab::General,
            super::settings::SettingsTab::Shortcuts,
        ]
        .into_iter()
        .enumerate()
        {
            fixture.route(cx, "dashboard");
            cx.simulate_window_resize(fixture.window, size(px(800.), px(700.)));
            fixture.render(cx);
            let mut visual = gpui_kit::VisualTestContext::from_window(fixture.window, cx);
            let panel = visual.debug_bounds("setup-checklist").expect("setup panel");
            assert!(panel.size.height <= px(220.));
            assert!(panel.origin.x + panel.size.width <= px(800.));
            for candidate in 0..3 {
                assert_eq!(
                    visual
                        .debug_bounds(["setup-step-0", "setup-step-1", "setup-step-2"][candidate])
                        .is_some(),
                    candidate == step
                );
            }
            let action = visual.debug_bounds("setup-action").unwrap();
            visual.simulate_click(action.center(), Default::default());
            drop(visual);
            fixture.render(cx);
            fixture.read(cx, |view, _| assert!(view.settings_ui.tab == tab));
            fixture.route(cx, "dashboard");
            fixture.read(cx, |view, _| assert_eq!(view.dashboard.setup_step, step));
            let mut visual = gpui_kit::VisualTestContext::from_window(fixture.window, cx);
            let next = visual.debug_bounds("setup-next").unwrap();
            visual.simulate_click(next.center(), Default::default());
            drop(visual);
            fixture.render(cx);
        }
        fixture.read(cx, |view, _| assert!(view.dashboard.setup_dismissed));
        let mut visual = gpui_kit::VisualTestContext::from_window(fixture.window, cx);
        assert!(visual.debug_bounds("setup-checklist").is_none());
        let reopen = visual.debug_bounds("setup-reopen").unwrap();
        visual.simulate_click(reopen.center(), Default::default());
        drop(visual);
        fixture.render(cx);
        let mut visual = gpui_kit::VisualTestContext::from_window(fixture.window, cx);
        let back = visual.debug_bounds("setup-back").unwrap();
        visual.simulate_click(back.center(), Default::default());
        drop(visual);
        fixture.render(cx);
        fixture.read(cx, |view, _| assert_eq!(view.dashboard.setup_step, 1));
    }
}
#[gpui_kit::test]
fn hosted_account_usage_renders_at_narrow_width_and_clears_on_disconnect(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    fixture.route(cx, "settings-provider");
    cx.simulate_window_resize(fixture.window, size(px(800.), px(600.)));
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        fixture.update(cx, |view, window, cx| {
            Theme::change(theme, Some(window), cx);
            view.page = Page::Settings;
            view.settings_ui.tab = super::settings::SettingsTab::Provider;
            view.settings.provider_mode = skillissue_domain::models::ProviderMode::Palepale;
            view.settings_ui.provider_mode = skillissue_domain::models::ProviderMode::Palepale;
            view.settings_ui.has_palepale_key = true;
            view.settings_ui.palepale_account_result = Some(Ok(skillissue_core::PalepaleAccountStatus {
                can_spend: true,
                details: Some(serde_json::from_value(serde_json::json!({
                    "available_microdollars":4949716,"reserved_microdollars":41833,
                    "passes":[{"plan":"Monthly","ends_at":"2026-11-19T00:00:00Z",
                    "allowance_microdollars":10000000,"remaining_microdollars":9000000,
                    "reserved_microdollars":100000,"scope":["text","image"]}]
                })).unwrap()),
                usage: Some(serde_json::from_value(serde_json::json!({
                    "days":0,"requests":26,"confirmed_requests":23,"unknown_requests":3,
                    "spent_microdollars":8451,"prompt_tokens":122000,"completion_tokens":7800,
                    "cached_tokens":31900,"reasoning_tokens":857
                })).unwrap()),
                usage_error: None,
            }));
            cx.notify();
        });
        fixture.render(cx);
        let mut visual = gpui_kit::VisualTestContext::from_window(fixture.window, cx);
        let panel = visual
            .debug_bounds("palepale-usage-panel")
            .expect("account panel laid out");
        assert!(panel.size.width > px(0.));
        assert!(panel.origin.x + panel.size.width <= px(800.));
        assert!(
            panel.size.height <= px(300.),
            "compact panel grew: {panel:?}"
        );
        let chart = visual
            .debug_bounds("palepale-token-chart")
            .expect("token chart");
        assert!(chart.origin.x >= panel.origin.x);
        assert!(chart.origin.x + chart.size.width <= panel.origin.x + panel.size.width);
        assert!(visual.debug_bounds("palepale-account-details").is_none());
        let toggle = visual
            .debug_bounds("palepale-details-toggle")
            .expect("details toggle");
        visual.simulate_click(toggle.center(), Default::default());
        drop(visual);
        fixture.render(cx);
        fixture.read(cx, |view, _| assert!(view.settings_ui.palepale_details));
        let mut visual = gpui_kit::VisualTestContext::from_window(fixture.window, cx);
        assert!(visual.debug_bounds("palepale-account-details").is_some());
        let toggle = visual.debug_bounds("palepale-details-toggle").unwrap();
        visual.simulate_click(toggle.center(), Default::default());
        drop(visual);
        fixture.render(cx);
        fixture.read(cx, |view, _| assert!(!view.settings_ui.palepale_details));
    }
    fixture.update(cx, |view, _, _| {
        view.cancel_palepale_account();
        assert!(view.settings_ui.palepale_account_result.is_none());
    });
}

impl Fixture {
    fn new(cx: &mut TestAppContext) -> Self {
        Self::with_resizing(cx, true)
    }

    fn with_resizing(cx: &mut TestAppContext, is_resizable: bool) -> Self {
        // TestPlatform supplies NoopTextSystem and TestWindow, never an HWND.
        // A current-thread runtime has no worker, and the inert constructor
        // does not boot an engine, register shortcuts, or start the UI timer.
        let runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("test runtime"),
        );
        let unused_data_dir =
            std::env::temp_dir().join(format!("skillissue-ui-test-{}", uuid::Uuid::new_v4()));
        let (window, view) = cx.update(|cx| {
            if !cx.has_global::<Theme>() {
                gpui_kit::init(cx);
            }
            Theme::change(ThemeMode::Light, None, cx);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: point(px(0.), px(0.)),
                        size: size(px(1100.), px(800.)),
                    })),
                    focus: false,
                    is_resizable,
                    titlebar: None,
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    cx.new(|cx| {
                        let mut view =
                            NativeView::new_for_test(runtime, unused_data_dir.clone(), window, cx);
                        view.gallery = true;
                        view.settings.theme = "light".into();
                        view.status = "Ready".into();
                        view
                    })
                },
            )
            .expect("open GPUI mock window")
        });
        let fixture = Self {
            window,
            view,
            unused_data_dir,
        };
        fixture.render(cx);
        cx.update_window(window, |_, window, cx| window.focus_next(cx))
            .expect("focus a mock control");
        fixture
    }

    fn update<R>(
        &self,
        cx: &mut TestAppContext,
        update: impl FnOnce(&mut NativeView, &mut Window, &mut Context<NativeView>) -> R,
    ) -> R {
        cx.update_window(self.window, |_, window, cx| {
            self.view.update(cx, |view, cx| update(view, window, cx))
        })
        .expect("update GPUI mock window")
    }

    fn read<R>(&self, cx: &TestAppContext, read: impl FnOnce(&NativeView, &App) -> R) -> R {
        self.view.read_with(cx, read)
    }

    fn render(&self, cx: &mut TestAppContext) {
        cx.update_window(self.window, |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
            // TestPlatform has no OS frame loop. Deliver a frame explicitly so
            // modal backdrop failures and deferred focus both complete.
            window.simulate_next_frame(cx);
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .expect("render GPUI mock frame");
        cx.run_until_parked();
    }

    fn route(&self, cx: &mut TestAppContext, id: &str) {
        self.update(cx, |view, window, cx| view.select_review(id, window, cx));
        self.render(cx);
    }

    fn press(&self, cx: &mut TestAppContext, key: &str) {
        self.event(cx, key, false, false);
    }

    fn type_text(&self, cx: &mut TestAppContext, text: &str) {
        cx.update_window(self.window, |_, window, cx| {
            for character in text.chars() {
                let text = character.to_string();
                let mut key = Keystroke::parse(&text).expect("test text character");
                key.key_char = Some(text);
                window.dispatch_keystroke(key, cx);
            }
        })
        .expect("type through the mock input handler");
        self.render(cx);
    }

    fn event(&self, cx: &mut TestAppContext, key: &str, is_held: bool, character_input: bool) {
        let keystroke = Keystroke::parse(key).expect("test keystroke");
        cx.update_window(self.window, |_, window, cx| {
            window.dispatch_event(
                KeyDownEvent {
                    keystroke: keystroke.clone(),
                    is_held,
                    prefer_character_input: character_input,
                }
                .to_platform_input(),
                cx,
            );
            window.dispatch_event(KeyUpEvent { keystroke }.to_platform_input(), cx);
        })
        .expect("dispatch mock keyboard input");
        self.render(cx);
    }

    fn shortcut(&self, cx: &TestAppContext, action: &str) -> String {
        self.read(cx, |view, cx| {
            view.shortcuts[action].read(cx).value().to_string()
        })
    }

    fn record(&self, cx: &mut TestAppContext, action: &str) {
        self.route(cx, "settings-keybinds");
        self.update(cx, |view, _, cx| {
            view.recording_key = Some(action.into());
            cx.notify();
        });
    }
}

#[gpui_kit::test]
fn fixture_has_no_engine_native_handle_services_or_disk_profile(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    fixture.read(cx, |view, _| {
        assert!(view.engine.is_none());
        assert!(view.native.is_none());
        assert!(view.hotkeys.is_none());
        assert!(view.tray.is_none());
        assert!(view.events.is_none());
        assert!(!view.opening);
        assert_eq!(view.ticks, 0);
    });
    assert!(!fixture.unused_data_dir.exists());
}

#[gpui_kit::test]
fn all_review_routes_render_in_both_themes_and_resize_bounds(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    assert_eq!(catalog::REVIEW_SCREENS.len(), 14);
    for theme in ["light", "dark"] {
        fixture.update(cx, |view, window, cx| {
            view.settings.theme = theme.into();
            Theme::change(
                if theme == "dark" {
                    ThemeMode::Dark
                } else {
                    ThemeMode::Light
                },
                Some(window),
                cx,
            );
        });
        for (width, height) in [(1100., 800.), (800., 600.)] {
            cx.simulate_window_resize(fixture.window, size(px(width), px(height)));
            fixture.update(cx, |_, window, _| {
                assert_eq!(window.viewport_size(), size(px(width), px(height)));
            });
            for screen in catalog::REVIEW_SCREENS {
                fixture.route(cx, screen.id);
                fixture.read(cx, |view, _| {
                    assert_eq!(view.review_id, screen.id);
                    assert!(view.error.is_none(), "{}: {:?}", screen.id, view.error);
                    assert!(!view.capturing_backdrop, "stuck material: {}", screen.id);
                });
            }
        }
    }
    fixture.read(cx, |view, _| {
        let expected: BTreeSet<_> = catalog::REVIEW_SCREENS
            .iter()
            .map(|screen| screen.id.to_owned())
            .collect();
        assert_eq!(view.rendered_screens, expected);
        assert_eq!(view.ticks, 0, "inert fixture must not run the app poller");
    });
    assert!(!fixture.unused_data_dir.exists());
}

#[gpui_kit::test]
fn gallery_navigation_clears_recorder_popups_and_scroll(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    fixture.route(cx, "settings-keybinds");
    fixture.update(cx, |view, _, _| {
        view.recording_key = Some("ask".into());
        view.preview_hidden = true;
        view.overlay_ui.menu = true;
        view.overlay_ui.modes = true;
        view.overlay_ui.tools = true;
        view.gallery_scroll.set_offset(point(px(0.), px(-200.)));
    });
    fixture.route(cx, "dashboard");
    fixture.read(cx, |view, _| {
        assert_eq!(view.review_id, "dashboard");
        assert!(view.recording_key.is_none());
        assert!(!view.preview_hidden);
        assert!(!view.overlay_ui.menu);
        assert!(!view.overlay_ui.modes);
        assert!(!view.overlay_ui.tools);
        assert_eq!(view.gallery_scroll.offset(), point(px(0.), px(0.)));
    });
}

#[gpui_kit::test]
fn unknown_review_route_preserves_the_active_screen_and_recorder(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    fixture.record(cx, "ask");
    fixture.route(cx, "screen-that-does-not-exist");
    fixture.read(cx, |view, _| {
        assert_eq!(view.review_id, "settings-keybinds");
        assert_eq!(view.recording_key.as_deref(), Some("ask"));
        assert!(view.page == Page::Keybinds);
    });
}

#[gpui_kit::test]
fn shortcut_escape_cancels_without_changing_the_binding(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    let original = fixture.shortcut(cx, "ask");
    fixture.record(cx, "ask");
    fixture.press(cx, "escape");
    assert_eq!(fixture.shortcut(cx, "ask"), original);
    assert!(fixture.read(cx, |view, _| view.recording_key.is_none()));
}

#[gpui_kit::test]
fn shortcut_delete_and_backspace_clear_only_the_selected_binding(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    for key in ["delete", "backspace"] {
        fixture.update(cx, |view, window, cx| {
            for (action, value) in default_shortcuts() {
                view.shortcuts[&action].update(cx, |input, cx| input.set_value(value, window, cx));
            }
        });
        fixture.record(cx, "ask");
        fixture.press(cx, key);
        assert!(fixture.shortcut(cx, "ask").is_empty());
        assert_eq!(fixture.shortcut(cx, "clear"), default_shortcuts()["clear"]);
        fixture.read(cx, |view, _| {
            assert!(view.recording_key.is_none());
            assert!(view.error.is_none());
            assert_eq!(view.shortcuts.len(), 11);
        });
    }
}

#[gpui_kit::test]
fn shortcut_repeat_composition_and_modifier_keys_do_not_commit(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    let original = fixture.shortcut(cx, "ask");
    fixture.record(cx, "ask");
    fixture.event(cx, "ctrl-alt-f12", true, false);
    fixture.event(cx, "ctrl-alt-f12", false, true);
    fixture.press(cx, "shift");
    assert_eq!(fixture.shortcut(cx, "ask"), original);
    assert_eq!(
        fixture.read(cx, |view, _| view.recording_key.clone()),
        Some("ask".into())
    );
}

#[gpui_kit::test]
fn shortcut_conflicts_keep_recording_and_preserve_all_values(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    let original = fixture.shortcut(cx, "ask");
    for key in ["ctrl-r", "ctrl-shift-,"] {
        fixture.record(cx, "ask");
        fixture.press(cx, key);
        fixture.read(cx, |view, _| {
            assert_eq!(view.recording_key.as_deref(), Some("ask"));
            assert!(
                view.error
                    .as_deref()
                    .is_some_and(|error| error.contains("same"))
            );
        });
        assert_eq!(fixture.shortcut(cx, "ask"), original);
    }
    fixture.press(cx, "escape");
}

#[gpui_kit::test]
fn shortcut_valid_commit_updates_the_ui_and_queues_persistence(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    fixture.record(cx, "ask");
    fixture.press(cx, "ctrl-alt-f12");
    let recorded = fixture.shortcut(cx, "ask");
    assert_eq!(
        skillissue_core::shortcuts::parse(&recorded)
            .expect("recorded shortcut")
            .id(),
        skillissue_core::shortcuts::parse("Ctrl+Alt+F12")
            .expect("expected shortcut")
            .id()
    );
    fixture.read(cx, |view, _| {
        assert!(view.recording_key.is_none());
        assert!(view.error.is_none());
        assert_eq!(view.settings.shortcuts["ask"], recorded);
        assert_eq!(view.preference_queue.len(), 1);
        assert!(
            !view.preference_saving,
            "no engine is attached to this fixture"
        );
    });
    assert_eq!(fixture.shortcut(cx, "clear"), default_shortcuts()["clear"]);
    assert!(!fixture.unused_data_dir.exists());
}

#[gpui_kit::test]
fn settings_shortcut_can_be_customized_cleared_and_restored(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    fixture.record(cx, "settings");
    fixture.press(cx, "ctrl-alt-f11");
    let recorded = fixture.shortcut(cx, "settings");
    assert_eq!(
        skillissue_core::shortcuts::parse(&recorded).unwrap().id(),
        skillissue_core::shortcuts::parse("Ctrl+Alt+F11")
            .unwrap()
            .id()
    );
    fixture.read(cx, |view, _| {
        assert_eq!(view.settings.shortcuts["settings"], recorded);
        assert!(view.recording_key.is_none());
    });
    fixture.record(cx, "settings");
    fixture.press(cx, "delete");
    assert!(fixture.shortcut(cx, "settings").is_empty());
    fixture.update(cx, |view, window, cx| {
        assert!(view.apply_shortcut_bindings(default_shortcuts(), window, cx));
    });
    assert_eq!(
        fixture.shortcut(cx, "settings"),
        default_shortcuts()["settings"]
    );
}

#[gpui_kit::test]
fn shortcut_reset_conflict_preserves_bindings_and_recording(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    fixture.update(cx, |view, window, cx| {
        let mut values = default_shortcuts();
        values.insert("settings".into(), "Ctrl+Alt+F11".into());
        values.insert("ask".into(), default_shortcuts()["settings"].clone());
        assert!(view.apply_shortcut_bindings(values.clone(), window, cx));
        view.recording_key = Some("settings".into());
        assert!(!view.set_shortcut_binding(
            "settings",
            default_shortcuts()["settings"].clone(),
            window,
            cx
        ));
        assert_eq!(view.settings.shortcuts, values);
        assert_eq!(view.recording_key.as_deref(), Some("settings"));
        assert!(
            view.error
                .as_deref()
                .is_some_and(|error| error.contains("same"))
        );
    });
}

#[gpui_kit::test]
fn shortcut_startup_does_not_cache_an_unregistered_state(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    fixture.update(cx, |view, _, cx| {
        assert!(view.hotkeys.is_none());
        assert!(!view.reconcile_global_shortcuts(cx));
        assert!(view.shortcut_state.is_none());
        for action in default_shortcuts().keys() {
            assert!(skillissue_core::shortcuts::enabled(
                action,
                &view.settings,
                false,
                false,
                false
            ));
        }
        view.recording_key = Some("settings".into());
        let context = view.global_shortcut_context(cx);
        for action in default_shortcuts().keys() {
            assert!(!context.enabled(ShortcutAction::from_name(action).unwrap()));
        }
    });
}

#[gpui_kit::test]
fn search_keyboard_open_close_and_escape_work_through_the_render_tree(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    fixture.route(cx, "dashboard");
    assert!(!fixture.read(cx, |view, _| view.dashboard_modal_open()));
    fixture.press(cx, "ctrl-k");
    assert!(fixture.read(cx, |view, _| view.dashboard_modal_open()));
    fixture.update(cx, |view, window, cx| {
        assert!(view.search.focus_handle(cx).is_focused(window));
    });
    fixture.press(cx, "escape");
    assert!(!fixture.read(cx, |view, _| view.dashboard_modal_open()));
    fixture.press(cx, "ctrl-k");
    fixture.press(cx, "ctrl-k");
    assert!(!fixture.read(cx, |view, _| view.dashboard_modal_open()));
}

#[gpui_kit::test]
fn overlay_menu_submenu_arrows_and_escape_close_without_changing_preferences(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx);
    fixture.route(cx, "chat");
    fixture.update(cx, |view, _, cx| {
        view.overlay_ui.menu = true;
        cx.notify();
    });
    let original = fixture.read(cx, |view, _| {
        (
            view.settings.capture_protection,
            view.settings.no_focus,
            view.settings.active_mode_id.clone(),
        )
    });
    fixture.press(cx, "down");
    fixture.press(cx, "up");
    fixture.press(cx, "right");
    assert!(fixture.read(cx, |view, _| view.overlay_ui.modes));
    fixture.press(cx, "down");
    fixture.press(cx, "up");
    fixture.press(cx, "left");
    assert!(!fixture.read(cx, |view, _| view.overlay_ui.modes));
    assert!(fixture.read(cx, |view, _| view.overlay_ui.menu));
    fixture.press(cx, "escape");
    fixture.read(cx, |view, _| {
        assert!(!view.overlay_ui.menu);
        assert!(!view.overlay_ui.modes);
        assert!(!view.overlay_ui.tools);
        assert_eq!(
            (
                view.settings.capture_protection,
                view.settings.no_focus,
                view.settings.active_mode_id.clone()
            ),
            original
        );
    });
}

#[gpui_kit::test]
fn overlay_mode_keyboard_can_reach_manage_modes(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    fixture.route(cx, "chat");
    fixture.update(cx, |view, _, cx| {
        view.overlay_ui.menu = true;
        cx.notify();
    });
    fixture.press(cx, "right");
    fixture.press(cx, "up");
    fixture.press(cx, "enter");
    fixture.read(cx, |view, _| {
        assert!(view.page == Page::Modes);
        assert_eq!(view.review_id, "modes");
        assert!(!view.overlay_ui.menu);
        assert!(!view.overlay_ui.modes);
    });
    fixture.render(cx);
    let mut visual = gpui_kit::VisualTestContext::from_window(fixture.window, cx);
    let selector = visual
        .debug_bounds("mode-selector")
        .expect("top mode selector");
    let editor = visual
        .debug_bounds("modes-content-scroll")
        .expect("full-width editor");
    assert!(selector.bottom() <= editor.top());
    assert!(editor.size.width >= selector.size.width);
    drop(visual);
    fixture.press(cx, "escape");
    assert!(fixture.read(cx, |view, _| view.page == Page::Sessions));
}

#[gpui_kit::test]
fn overlay_child_returns_to_live_after_opening_parent_modes(cx: &mut TestAppContext) {
    let parent = Fixture::new(cx);
    let child = Fixture::new(cx);
    let parent_weak = parent.update(cx, |_, _, cx| cx.weak_entity());
    child.update(cx, |view, _, cx| {
        view.gallery = false;
        view.is_overlay = true;
        view.page = Page::Modes;
        view.parent = Some((parent.window, parent_weak));
        view.show_dashboard(cx);
        assert!(view.page == Page::Live);
    });
    parent.read(cx, |view, _| {
        assert!(view.page == Page::Modes);
        assert!(view.dashboard_modal_open());
    });
    child.update(cx, |view, _, cx| view.show_dashboard(cx));
    parent.read(cx, |view, _| {
        assert!(view.page == Page::Sessions);
        assert!(!view.dashboard_modal_open());
    });
    child.read(cx, |view, _| {
        assert!(view.page == Page::Live);
        assert!(view.native.is_none());
    });
    assert!(!parent.unused_data_dir.exists());
    assert!(!child.unused_data_dir.exists());
}

#[gpui_kit::test]
fn session_shortcuts_reveal_parent_settings_and_preserve_child_surface(cx: &mut TestAppContext) {
    let parent = Fixture::new(cx);
    let child = Fixture::new(cx);
    let parent_weak = parent.update(cx, |view, _, cx| {
        view.shown = false;
        view.preview_hidden = true;
        view.overlay = Some((child.window, child.view.clone()));
        cx.weak_entity()
    });
    child.update(cx, |view, _, cx| {
        view.gallery = false;
        view.is_overlay = true;
        view.page = Page::Live;
        view.overlay_ui.menu = true;
        view.parent = Some((parent.window, parent_weak));
        view.activate_overlay_menu(0, cx);
        assert!(view.page == Page::Live);
        assert!(!view.overlay_ui.menu);
    });
    parent.update(cx, |view, window, cx| {
        assert!(view.page == Page::Keybinds);
        assert!(view.settings_ui.tab == super::settings::SettingsTab::Shortcuts);
        assert!(view.shown && !view.preview_hidden);
        assert!(view.root_focus.is_focused(window));
        view.change_preference(|settings| settings.capture_protection = true);
        assert!(view.shown);
        cx.notify();
    });
    parent.render(cx);
    child.render(cx);
}

#[gpui_kit::test]
fn prompt_effort_picker_replaces_smart_and_updates_reasoning(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    fixture.route(cx, "chat");
    fixture.update(cx, |view, _, cx| {
        view.settings.smart_mode = false;
        cx.notify();
    });
    fixture.render(cx);
    let mut visual = gpui_kit::VisualTestContext::from_window(fixture.window, cx);
    assert!(visual.debug_bounds("overlay-smart-button").is_none());
    let picker = visual
        .debug_bounds("overlay-effort-picker")
        .expect("effort picker");
    visual.simulate_click(picker.center(), Default::default());
    drop(visual);
    cx.simulate_keystrokes(fixture.window, "down down down enter");
    fixture.read(cx, |view, _| {
        assert!(view.settings.smart_mode);
        assert_eq!(view.settings.smart_effort, "high");
    });
}

#[gpui_kit::test]
fn home_dropdown_stays_compact(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    fixture.route(cx, "dashboard");
    fixture.render(cx);
    let mut visual = gpui_kit::VisualTestContext::from_window(fixture.window, cx);
    let trigger = visual.debug_bounds("dashboard-menu").unwrap();
    visual.simulate_click(trigger.center(), Default::default());
    drop(visual);
    fixture.render(cx);
    let mut visual = gpui_kit::VisualTestContext::from_window(fixture.window, cx);
    let menu = visual.debug_bounds("workspace-menu-panel").unwrap();
    assert!(menu.size.width <= px(152.));
    assert!(menu.size.height <= px(68.));
    let settings = visual.debug_bounds("workspace-settings").unwrap();
    visual.simulate_click(settings.center(), Default::default());
    drop(visual);
    fixture.read(cx, |view, _| assert!(view.page == Page::Settings));
}

#[gpui_kit::test]
fn capture_foil_preserves_toggle_hit_target_and_visibility(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    fixture.route(cx, "dashboard");
    fixture.update(cx, |view, _, cx| {
        view.shown = true;
        view.settings.capture_protection = false;
        cx.notify();
    });
    for enabled in [true, false] {
        fixture.render(cx);
        let mut visual = gpui_kit::VisualTestContext::from_window(fixture.window, cx);
        let button = visual
            .debug_bounds("capture-status")
            .expect("capture toggle");
        assert_eq!(button.size.height, px(super::style::CONTROL_HEIGHT));
        visual.simulate_click(button.center(), Default::default());
        drop(visual);
        fixture.read(cx, |view, _| {
            assert_eq!(view.settings.capture_protection, enabled);
            assert!(view.shown);
        });
    }
}

#[gpui_kit::test]
fn overlay_popups_render_at_narrow_and_expanded_mock_window_sizes(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    fixture.update(cx, |view, _, cx| {
        view.gallery = false;
        view.is_overlay = true;
        view.page = Page::Live;
        cx.notify();
    });
    for (width, height) in [(183., 220.), (320., 550.), (560., 550.)] {
        cx.simulate_window_resize(fixture.window, size(px(width), px(height)));
        fixture.update(cx, |view, _, cx| {
            view.overlay_ui.menu = true;
            view.overlay_ui.modes = true;
            view.overlay_ui.tools = false;
            cx.notify();
        });
        fixture.render(cx);
        fixture.update(cx, |view, window, cx| {
            assert_eq!(window.viewport_size(), size(px(width), px(height)));
            assert!(view.overlay_ui.menu);
            assert!(view.overlay_ui.modes);
            assert!(view.error.is_none());
            view.overlay_ui.menu = false;
            view.overlay_ui.modes = false;
            view.overlay_ui.tools = true;
            cx.notify();
        });
        fixture.render(cx);
        fixture.read(cx, |view, _| {
            assert!(view.overlay_ui.tools);
            assert!(view.native.is_none());
        });
    }
    assert!(!fixture.unused_data_dir.exists());
}

#[gpui_kit::test]
fn provider_configuration_refresh_preserves_unsaved_fields_and_key(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    fixture.update(cx, |view, _, _| view.settings_ui.byok_advanced = true);
    fixture.route(cx, "settings-provider");
    fixture.update(cx, |view, window, cx| view.sync_provider_config(window, cx));
    fixture.update(cx, |view, window, cx| {
        view.endpoint
            .update(cx, |input, cx| input.set_value("", window, cx));
        view.endpoint.focus_handle(cx).focus(window, cx);
    });
    fixture.type_text(cx, "http://127.0.0.1:4567/v1");
    fixture.update(cx, |view, window, cx| {
        view.api_key.focus_handle(cx).focus(window, cx);
    });
    fixture.type_text(cx, "local-unsaved-key");
    fixture.update(cx, |view, window, cx| {
        view.settings_ui
            .provider_prompt
            .update(cx, |input, cx| input.set_value("", window, cx));
        view.settings_ui
            .provider_prompt
            .focus_handle(cx)
            .focus(window, cx);
    });
    fixture.type_text(cx, "Unsaved local prompt");
    fixture.update(cx, |view, window, cx| {
        let mut settings = view.settings.clone();
        settings.api_base_url = "http://127.0.0.1:9999/v1".into();
        settings.chat_model = "refreshed-model".into();
        settings.system_prompt = "Refreshed saved prompt".into();
        view.set_config(
            ConfigView {
                settings,
                has_api_key: false,
                has_byok_key: false,
                has_audio_key: false,
                has_image_key: false,
                has_palepale_key: false,
                platform: "test".into(),
                shortcuts: vec![],
                warnings: vec![],
            },
            window,
            cx,
        );
    });
    fixture.read(cx, |view, cx| {
        assert_eq!(
            view.endpoint.read(cx).value().as_str(),
            "http://127.0.0.1:4567/v1"
        );
        assert_eq!(view.api_key.read(cx).value().as_str(), "local-unsaved-key");
        assert_eq!(
            view.settings_ui.provider_prompt.read(cx).value().as_str(),
            "Unsaved local prompt"
        );
        assert_eq!(view.model.read(cx).value().as_str(), "refreshed-model");
    });
}

#[gpui_kit::test]
fn older_config_acknowledgment_keeps_two_pending_preference_changes(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    fixture.update(cx, |view, window, cx| {
        let confirmed = view.settings.clone();
        let hide_widget = !confirmed.hide_widget;
        let smart_mode = !confirmed.smart_mode;
        view.change_preference(|settings| settings.hide_widget = hide_widget);
        view.change_preference(|settings| {
            settings.smart_mode = smart_mode;
            settings.smart_effort = "low".into();
        });
        view.set_config(
            ConfigView {
                settings: confirmed,
                has_api_key: false,
                has_byok_key: false,
                has_audio_key: false,
                has_image_key: false,
                has_palepale_key: false,
                platform: "test".into(),
                shortcuts: vec![],
                warnings: vec![],
            },
            window,
            cx,
        );
        assert_eq!(view.settings.hide_widget, hide_widget);
        assert_eq!(view.settings.smart_mode, smart_mode);
        assert_eq!(view.settings.smart_effort, "low");
        assert_eq!(view.preference_queue.len(), 2);
        assert!(!view.preference_saving);
    });
    assert!(!fixture.unused_data_dir.exists());
}

#[gpui_kit::test]
fn clear_cancels_the_response_and_preserves_gallery_window_bounds(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    fixture.route(cx, "chat");
    let cancelled = tokio_util::sync::CancellationToken::new();
    fixture.update(cx, |view, window, cx| {
        let bounds = window.window_bounds();
        view.request_cancel = Some(cancelled.clone());
        view.question
            .update(cx, |input, cx| input.set_value("Unsent draft", window, cx));
        view.stream = "Partial response".into();
        view.error = Some("Earlier provider error".into());
        view.expanded = true;
        view.overlay_action(ShortcutAction::Clear, window, cx);
        assert_eq!(window.window_bounds(), bounds);
        assert!(view.question.read(cx).value().is_empty());
        assert!(view.stream.is_empty());
        assert!(view.error.is_none());
        assert!(!view.expanded);
    });
    assert!(cancelled.is_cancelled());
}

#[gpui_kit::test]
fn composer_shift_enter_keeps_editing_and_plain_enter_submits(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    fixture.route(cx, "chat");
    fixture.update(cx, |view, window, cx| {
        view.question
            .update(cx, |input, cx| input.set_value("Draft", window, cx));
        view.begin_typing(window, cx);
    });
    fixture.press(cx, "shift-enter");
    fixture.read(cx, |view, cx| {
        assert!(view.question.read(cx).value().contains('\n'));
        assert!(view.typing);
        assert!(!view.expanded);
    });
    fixture.press(cx, "enter");
    fixture.read(cx, |view, _| {
        assert!(!view.typing);
        assert!(view.expanded);
        assert!(view.overlay_ui.chat_shown);
        assert!(!view.busy, "no engine is attached to this fixture");
    });
}

#[gpui_kit::test]
fn fixed_overlay_still_hides_shows_expands_and_clears(cx: &mut TestAppContext) {
    let fixture = Fixture::with_resizing(cx, false);
    fixture.update(cx, |view, window, cx| {
        view.gallery = false;
        view.is_overlay = true;
        view.settings.hide_widget = false;
        view.overlay_ui.chat_shown = true;
        view.expanded = false;
        assert!(!window.is_resizable());
        view.toggle_chat(window, cx);
        assert!(!view.overlay_ui.chat_shown);
        assert_eq!(
            window.window_bounds().get_bounds().size,
            size(px(183.), px(70.))
        );
        view.toggle_chat(window, cx);
        assert!(view.overlay_ui.chat_shown);
        assert_eq!(
            window.window_bounds().get_bounds().size,
            size(px(560.), px(256.))
        );
        view.expanded = true;
        view.reveal_overlay_window(true, window, cx);
        assert_eq!(
            window.window_bounds().get_bounds().size,
            size(px(560.), px(550.))
        );
        view.toggle_chat(window, cx);
        view.toggle_chat(window, cx);
        assert_eq!(
            window.window_bounds().get_bounds().size,
            size(px(560.), px(550.))
        );
        view.overlay_action(ShortcutAction::Clear, window, cx);
        assert_eq!(
            window.window_bounds().get_bounds().size,
            size(px(560.), px(256.))
        );
        assert!(!window.is_resizable());
    });
}

#[gpui_kit::test]
fn overlay_hide_show_button_accepts_repeated_mouse_clicks(cx: &mut TestAppContext) {
    let fixture = Fixture::with_resizing(cx, false);
    fixture.update(cx, |view, _, _| {
        view.gallery = false;
        view.is_overlay = true;
        view.page = Page::Live;
        view.settings.hide_widget = false;
        view.overlay_ui.chat_shown = true;
    });
    for expected in [false, true, false, true] {
        fixture.render(cx);
        let mut visual = gpui_kit::VisualTestContext::from_window(fixture.window, cx);
        let bounds = visual
            .debug_bounds("overlay-toggle-chat")
            .expect("toggle button");
        visual.simulate_click(bounds.center(), Default::default());
        fixture.read(cx, |view, _| {
            assert_eq!(view.overlay_ui.chat_shown, expected)
        });
    }
}

#[gpui_kit::test]
fn hidden_chat_stays_hidden_when_a_response_finishes(cx: &mut TestAppContext) {
    let fixture = Fixture::with_resizing(cx, false);
    fixture.update(cx, |view, window, cx| {
        view.gallery = false;
        view.is_overlay = true;
        view.page = Page::Live;
        view.settings.hide_widget = false;
        view.overlay_ui.chat_shown = true;
        view.overlay_ui.menu = true;
        view.overlay_ui.tools = true;
        view.overlay_ui.transcript = true;
        view.toggle_chat(window, cx);
        assert!(!view.overlay_ui.menu && !view.overlay_ui.tools && !view.overlay_ui.transcript);
        let hidden_bounds = window.window_bounds();
        view.busy = true;
        view.overlay_tick(window, cx);
        view.snapshot
            .messages
            .push(skillissue_core::models::Message {
                id: "completed".into(),
                role: "assistant".into(),
                content: "Answer".into(),
                created_at: 1,
            });
        view.busy = false;
        view.overlay_tick(window, cx);
        assert!(!view.overlay_ui.chat_shown);
        assert_eq!(window.window_bounds(), hidden_bounds);
        view.toggle_chat(window, cx);
        assert!(view.overlay_ui.chat_shown);
        assert_eq!(
            window.window_bounds().get_bounds().size,
            size(px(560.), px(550.))
        );
    });
}

#[gpui_kit::test]
fn provider_modes_keep_independent_drafts_and_only_clear_the_saved_credential(
    cx: &mut TestAppContext,
) {
    use skillissue_domain::{models::ProviderMode, provider::ProviderModel};
    let fixture = Fixture::new(cx);
    fixture.update(cx, |view, _, _| view.settings_ui.byok_advanced = true);
    fixture.route(cx, "settings-provider");
    fixture.update(cx, |view, window, cx| {
        view.sync_provider_config(window, cx);
        view.endpoint
            .update(cx, |input, cx| input.set_value("", window, cx));
        view.endpoint.focus_handle(cx).focus(window, cx);
    });
    fixture.type_text(cx, "http://127.0.0.1:4567/v1");
    fixture.update(cx, |view, window, cx| {
        view.api_key.focus_handle(cx).focus(window, cx)
    });
    fixture.type_text(cx, "byok-draft-secret");
    fixture.update(cx, |view, _, cx| {
        view.select_provider_mode(ProviderMode::Palepale, cx);
    });
    fixture.update(cx, |view, _, cx| {
        view.settings_ui.catalog_generation = 7;
        view.settings_ui.catalog_loading = true;
        view.receive_provider_catalog(
            7,
            Ok(vec![ProviderModel {
                id: "hosted-chat".into(),
                display_name: "Hosted Chat".into(),
                ..Default::default()
            }]),
        );
        view.select_palepale_model("hosted-chat", cx);
    });
    fixture.render(cx);
    fixture.update(cx, |view, window, cx| {
        view.select_provider_mode(ProviderMode::Byok, cx);
        view.select_provider_mode(ProviderMode::Palepale, cx);
        let confirmed = view.settings.clone();
        view.set_config(
            ConfigView {
                settings: confirmed,
                has_api_key: false,
                has_byok_key: false,
                has_audio_key: false,
                has_image_key: false,
                has_palepale_key: false,
                platform: "test".into(),
                shortcuts: vec![],
                warnings: vec![],
            },
            window,
            cx,
        );
        assert_eq!(view.settings.provider_mode, ProviderMode::Byok);
        assert_eq!(view.settings_ui.provider_mode, ProviderMode::Palepale);
        assert_eq!(view.settings_ui.palepale_model, "hosted-chat");
        assert_eq!(view.api_key.read(cx).value().as_str(), "byok-draft-secret");
        let mut saved = view.settings.clone();
        saved.provider_mode = ProviderMode::Palepale;
        saved.palepale.chat_model = "hosted-chat".into();
        view.settings_ui.provider_save_pending = true;
        view.sender
            .send(super::Reply::ProviderConfig(
                ProviderMode::Palepale,
                Ok(ConfigView {
                    settings: saved,
                    has_api_key: true,
                    has_byok_key: false,
                    has_audio_key: false,
                    has_image_key: false,
                    has_palepale_key: true,
                    platform: "test".into(),
                    shortcuts: vec![],
                    warnings: vec![],
                }),
            ))
            .unwrap();
        view.tick(window, cx);
        assert_eq!(view.settings.provider_mode, ProviderMode::Palepale);
        assert!(view.settings_ui.palepale_key.read(cx).value().is_empty());
        assert_eq!(view.api_key.read(cx).value().as_str(), "byok-draft-secret");
        view.select_provider_mode(ProviderMode::Byok, cx);
        assert_eq!(
            view.endpoint.read(cx).value().as_str(),
            "http://127.0.0.1:4567/v1"
        );
        assert!(view.provider_draft_dirty(cx));
    });
    fixture.render(cx);
    assert!(!fixture.unused_data_dir.exists());
}

#[gpui_kit::test]
fn provider_catalog_ignores_stale_results_and_mode_switches_during_save(cx: &mut TestAppContext) {
    use skillissue_domain::{models::ProviderMode, provider::ProviderModel};
    let fixture = Fixture::new(cx);
    fixture.route(cx, "settings-provider");
    fixture.update(cx, |view, _, cx| {
        view.select_provider_mode(ProviderMode::Palepale, cx);
        view.settings_ui.catalog_generation = 3;
        view.settings_ui.catalog_loading = true;
        view.receive_provider_catalog(
            2,
            Ok(vec![ProviderModel {
                id: "stale".into(),
                ..Default::default()
            }]),
        );
        assert!(view.settings_ui.catalog_models.is_empty());
        assert!(view.settings_ui.catalog_loading);
        view.receive_provider_catalog(
            3,
            Ok(vec![ProviderModel {
                id: "current".into(),
                ..Default::default()
            }]),
        );
        view.select_palepale_model("arbitrary-not-in-catalog", cx);
        assert!(view.settings_ui.palepale_model.is_empty());
        view.select_palepale_model("current", cx);
        view.settings_ui.provider_save_pending = true;
        view.select_provider_mode(ProviderMode::Byok, cx);
        view.select_palepale_model("stale", cx);
        assert_eq!(view.settings_ui.provider_mode, ProviderMode::Palepale);
        assert_eq!(view.settings_ui.palepale_model, "current");
        view.settings_ui.provider_save_pending = false;
        view.settings_ui.catalog_loading = true;
        view.select_provider_mode(ProviderMode::Byok, cx);
        view.receive_provider_catalog(3, Err("late failure".into()));
        assert!(view.settings_ui.catalog_error.is_none());
        assert_eq!(view.settings_ui.catalog_models[0].id, "current");
        assert!(!view.settings_ui.catalog_loading);
    });
    fixture.render(cx);
    assert!(!fixture.unused_data_dir.exists());
}

#[gpui_kit::test]
fn hosted_pairing_ignores_stale_start_and_cancels_pending_approval(cx: &mut TestAppContext) {
    use super::settings::PalepalePairingState;
    use skillissue_domain::models::ProviderMode;
    let fixture = Fixture::new(cx);
    fixture.route(cx, "settings-provider");
    fixture.update(cx, |view, window, cx| {
        view.select_provider_mode(ProviderMode::Palepale, cx);
        assert_eq!(
            view.settings_ui.palepale_pairing,
            PalepalePairingState::Idle
        );
        view.settings_ui.palepale_pairing_generation = 7;
        view.settings_ui.palepale_pairing = PalepalePairingState::Starting;
        view.receive_palepale_pairing_started(
            6,
            Ok((
                "stale".into(),
                "https://palepale.tech/pair?code=stale".into(),
                60,
            )),
            cx,
        );
        assert_eq!(
            view.settings_ui.palepale_pairing,
            PalepalePairingState::Starting
        );
        view.receive_palepale_pairing_started(
            7,
            Ok((
                "123456".into(),
                "https://palepale.tech/pair?code=123456".into(),
                60,
            )),
            cx,
        );
        assert!(matches!(
            view.settings_ui.palepale_pairing,
            PalepalePairingState::Waiting { ref code, .. } if code == "123456"
        ));
        view.cancel_palepale_pairing(cx);
        assert_eq!(
            view.settings_ui.palepale_pairing,
            PalepalePairingState::Idle
        );
        let mut late_settings = view.settings.clone();
        late_settings.provider_mode = ProviderMode::Palepale;
        view.receive_palepale_pairing_ready(
            7,
            Ok(ConfigView {
                settings: late_settings,
                has_api_key: true,
                has_byok_key: false,
                has_audio_key: false,
                has_image_key: false,
                has_palepale_key: true,
                platform: "test".into(),
                shortcuts: vec![],
                warnings: vec![],
            }),
            window,
            cx,
        );
        assert_eq!(view.settings.provider_mode, ProviderMode::Byok);
        assert!(!view.settings_ui.has_palepale_key);
        view.sync_provider_config(window, cx);
    });
    fixture.render(cx);
    assert!(!fixture.unused_data_dir.exists());
}

#[gpui_kit::test]
fn saved_hosted_pairing_error_remains_visible_during_catalog_outage(cx: &mut TestAppContext) {
    use super::settings::PalepalePairingState;
    use skillissue_domain::models::ProviderMode;
    let fixture = Fixture::new(cx);
    fixture.route(cx, "settings-provider");
    fixture.update(cx, |view, window, cx| {
        view.settings.provider_mode = ProviderMode::Palepale;
        view.sync_provider_config(window, cx);
        view.settings_ui.has_palepale_key = true;
        view.settings_ui.palepale_pairing = PalepalePairingState::Ready;
        assert!(matches!(
            view.settings_ui.palepale_pairing,
            PalepalePairingState::Ready
        ));
        view.settings_ui.palepale_pairing = PalepalePairingState::Error("expired".into());
        assert!(matches!(
            view.settings_ui.palepale_pairing,
            PalepalePairingState::Error(ref message) if message == "expired"
        ));
    });
    fixture.render(cx);
    assert!(!fixture.unused_data_dir.exists());
}

#[gpui_kit::test]
fn saved_hosted_configuration_requires_account_and_catalog_before_ready(cx: &mut TestAppContext) {
    use skillissue_domain::{models::ProviderMode, provider::ProviderModel};
    let fixture = Fixture::new(cx);
    fixture.update(cx, |view, window, cx| {
        let mut settings = view.settings.clone();
        settings.provider_mode = ProviderMode::Palepale;
        settings.palepale.chat_model = "hosted-chat".into();
        view.set_config(
            ConfigView {
                settings,
                has_api_key: true,
                has_byok_key: false,
                has_audio_key: false,
                has_image_key: false,
                has_palepale_key: true,
                platform: "test".into(),
                shortcuts: vec![],
                warnings: vec![],
            },
            window,
            cx,
        );
        assert_eq!(view.palepale_connection_status().0, "Connection saved");
        assert_eq!(
            view.settings_ui.palepale_pairing,
            super::settings::PalepalePairingState::Idle
        );
        view.settings_ui.catalog_generation = 7;
        view.settings_ui.catalog_loading = true;
        assert_eq!(view.palepale_connection_status().0, "Checking Palepale");
        view.receive_provider_catalog(
            7,
            Ok(vec![ProviderModel {
                id: "hosted-chat".into(),
                capabilities: skillissue_domain::provider::ModelCapabilities {
                    streaming: true,
                    ..Default::default()
                },
                ..Default::default()
            }]),
        );
        view.settings_ui.palepale_account_generation = 8;
        view.settings_ui.palepale_account_loading = true;
        assert_eq!(view.palepale_connection_status().0, "Checking Palepale");
        view.receive_palepale_account(
            8,
            Ok(skillissue_core::PalepaleAccountStatus {
                can_spend: true,
                ..Default::default()
            }),
        );
        assert_eq!(view.palepale_connection_status().0, "Palepale");
        view.settings_ui.palepale_account_loading = true;
        view.receive_palepale_account(
            8,
            Ok(skillissue_core::PalepaleAccountStatus {
                can_spend: false,
                ..Default::default()
            }),
        );
        assert_eq!(
            view.palepale_connection_status().0,
            "Account cannot make requests"
        );
        view.settings_ui.palepale_account_loading = true;
        view.receive_palepale_account(8, Err("Account check unavailable".into()));
        assert_eq!(view.palepale_connection_status().0, "Palepale unavailable");
        view.settings_ui.palepale_account_result =
            Some(Ok(skillissue_core::PalepaleAccountStatus {
                can_spend: true,
                ..Default::default()
            }));
        view.settings_ui.catalog_error = Some("Catalog unavailable".into());
        assert_eq!(view.palepale_connection_status().0, "Palepale unavailable");
        view.settings_ui.catalog_error = None;
        view.settings_ui.catalog_models.clear();
        assert_eq!(view.palepale_connection_status().0, "Model unavailable");
    });
    fixture.render(cx);
    assert!(!fixture.unused_data_dir.exists());
}

#[gpui_kit::test]
fn hosted_disconnect_and_mode_change_discard_late_readiness_results(cx: &mut TestAppContext) {
    use skillissue_domain::{models::ProviderMode, provider::ProviderModel};
    let fixture = Fixture::new(cx);
    fixture.update(cx, |view, _, cx| {
        view.settings.provider_mode = ProviderMode::Palepale;
        view.settings_ui.has_palepale_key = true;
        view.settings_ui.catalog_generation = 3;
        view.settings_ui.catalog_loading = true;
        view.settings_ui.palepale_account_generation = 4;
        view.settings_ui.palepale_account_loading = true;
        let catalog_cancel = tokio_util::sync::CancellationToken::new();
        let account_cancel = tokio_util::sync::CancellationToken::new();
        view.settings_ui.catalog_cancel = Some(catalog_cancel.clone());
        view.settings_ui.palepale_account_cancel = Some(account_cancel.clone());
        view.select_provider_mode(ProviderMode::Byok, cx);
        assert!(catalog_cancel.is_cancelled());
        assert!(account_cancel.is_cancelled());
        view.receive_provider_catalog(
            3,
            Ok(vec![ProviderModel {
                id: "late".into(),
                ..Default::default()
            }]),
        );
        view.receive_palepale_account(
            4,
            Ok(skillissue_core::PalepaleAccountStatus {
                can_spend: true,
                ..Default::default()
            }),
        );
        assert!(view.settings_ui.catalog_models.is_empty());
        assert!(view.settings_ui.palepale_account_result.is_none());
        view.settings_ui.catalog_loading = true;
        view.settings_ui.palepale_account_loading = true;
        let catalog_generation = view.settings_ui.catalog_generation;
        let account_generation = view.settings_ui.palepale_account_generation;
        view.disconnect_palepale(cx);
        view.receive_provider_catalog(catalog_generation, Err("late failure".into()));
        view.receive_palepale_account(
            account_generation,
            Ok(skillissue_core::PalepaleAccountStatus {
                can_spend: true,
                ..Default::default()
            }),
        );
        assert!(view.settings_ui.catalog_error.is_none());
        assert!(view.settings_ui.palepale_account_result.is_none());
        assert!(!view.settings_ui.catalog_loading);
        assert!(!view.settings_ui.palepale_account_loading);
    });
    assert!(!fixture.unused_data_dir.exists());
}

#[gpui_kit::test]
fn hosted_model_changes_and_disconnect_clear_verified_account_status(cx: &mut TestAppContext) {
    use skillissue_domain::{models::ProviderMode, provider::ProviderModel};
    let fixture = Fixture::new(cx);
    fixture.update(cx, |view, window, cx| {
        view.settings.provider_mode = ProviderMode::Palepale;
        view.settings.palepale.chat_model = "old".into();
        view.settings_ui.has_palepale_key = true;
        view.settings_ui.palepale_pairing = super::settings::PalepalePairingState::Ready;
        view.settings_ui.palepale_account_result =
            Some(Ok(skillissue_core::PalepaleAccountStatus {
                can_spend: true,
                ..Default::default()
            }));
        view.settings_ui.catalog_models = vec![ProviderModel {
            id: "old".into(),
            capabilities: skillissue_domain::provider::ModelCapabilities {
                streaming: true,
                ..Default::default()
            },
            ..Default::default()
        }];
        assert_eq!(view.palepale_connection_status().0, "Palepale");
        let mut settings = view.settings.clone();
        settings.palepale.chat_model = "new".into();
        let mut config = ConfigView {
            settings,
            has_api_key: true,
            has_byok_key: false,
            has_audio_key: false,
            has_image_key: false,
            has_palepale_key: true,
            platform: "test".into(),
            shortcuts: vec![],
            warnings: vec![],
        };
        view.set_config(config.clone(), window, cx);
        assert_eq!(view.palepale_connection_status().0, "Connection saved");
        assert!(view.settings_ui.palepale_account_result.is_none());
        config.has_api_key = false;
        config.has_palepale_key = false;
        view.set_config(config, window, cx);
        assert_eq!(view.palepale_connection_status().0, "Not connected");
        assert_eq!(
            view.settings_ui.palepale_pairing,
            super::settings::PalepalePairingState::Idle
        );
    });
    assert!(!fixture.unused_data_dir.exists());
}

#[gpui_kit::test]
fn hosted_drafts_do_not_disable_byok_and_committed_hosted_blocks_device_work(
    cx: &mut TestAppContext,
) {
    use skillissue_domain::models::ProviderMode;
    let fixture = Fixture::new(cx);
    fixture.update(cx, |view, _, cx| {
        view.select_provider_mode(ProviderMode::Palepale, cx);
        view.toggle_capture();
        assert!(view.error.is_none());
        view.settings.provider_mode = ProviderMode::Palepale;
        view.toggle_capture();
        assert!(
            view.error
                .as_deref()
                .unwrap()
                .contains("Audio check failed")
        );
        view.error = None;
        view.capture_preview();
        assert!(
            view.error
                .as_deref()
                .unwrap()
                .contains("Image check failed")
        );
        assert!(!view.busy);
    });
    fixture.route(cx, "chat");
    fixture.render(cx);
    assert!(!fixture.unused_data_dir.exists());
}

#[gpui_kit::test]
fn hosted_settings_and_chat_render_with_preserved_byok_capability_preferences(
    cx: &mut TestAppContext,
) {
    use skillissue_domain::{models::ProviderMode, provider::ProviderModel};
    let fixture = Fixture::new(cx);
    fixture.update(cx, |view, _, _| {
        view.settings.smart_mode = true;
        view.settings.include_screenshot = true;
        view.settings.provider_mode = ProviderMode::Palepale;
        view.settings.palepale.chat_model = "hosted-chat".into();
        view.settings_ui.catalog_models = vec![ProviderModel {
            id: "hosted-chat".into(),
            display_name: "Hosted Chat".into(),
            ..Default::default()
        }];
    });
    for (width, height) in [(1100., 800.), (800., 600.)] {
        cx.simulate_window_resize(fixture.window, size(px(width), px(height)));
        for dark in [false, true] {
            fixture.update(cx, |view, _, _| {
                view.settings.theme = if dark { "dark".into() } else { "light".into() }
            });
            fixture.route(cx, "settings-provider");
            fixture.update(cx, |view, window, cx| view.sync_provider_config(window, cx));
            fixture.render(cx);
            fixture.route(cx, "chat");
            fixture.read(cx, |view, _| {
                assert!(view.settings.smart_mode);
                assert!(view.settings.include_screenshot);
                assert!(view.settings.is_palepale());
                assert!(view.error.is_none());
                assert!(view.engine.is_none());
            });
        }
    }
    assert!(!fixture.unused_data_dir.exists());
}

#[gpui_kit::test]
fn hosted_composer_buttons_fit_inside_collapsed_and_expanded_native_and_gallery_panels(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx);
    for (gallery, width, height) in [
        (true, 1100., 800.),
        (true, 800., 600.),
        (false, 560., 244.),
        (false, 560., 550.),
    ] {
        cx.simulate_window_resize(fixture.window, size(px(width), px(height)));
        for protected in [false, true] {
            for expanded in [false, true] {
                if !gallery && height == 244. && expanded {
                    continue;
                }
                fixture.update(cx, |view, window, cx| {
                    view.settings.provider_mode = skillissue_domain::models::ProviderMode::Palepale;
                    view.settings.capture_protection = protected;
                    view.gallery = gallery;
                    view.is_overlay = !gallery;
                    view.page = Page::Live;
                    view.review_id = "chat".into();
                    view.expanded = expanded;
                    view.overlay_ui.chat_shown = true;
                    view.settings.smart_mode = true;
                    view.root_focus.focus(window, cx);
                    cx.notify();
                });
                fixture.render(cx);
                let mut visual = gpui_kit::VisualTestContext::from_window(fixture.window, cx);
                let panel = visual
                    .debug_bounds("overlay-chat-panel")
                    .expect("chat panel laid out");
                let header = visual
                    .debug_bounds("overlay-header-panel")
                    .expect("session header");
                let microphone = visual
                    .debug_bounds("session-recording-button")
                    .expect("microphone");
                let tools = visual
                    .debug_bounds("session-tools-trigger")
                    .expect("tools trigger");
                assert!(
                    microphone.right() <= tools.left(),
                    "header controls must not overlap"
                );
                assert_eq!(microphone.size.height, tools.size.height);
                assert!(tools.right() <= header.right());
                assert_eq!(
                    header.left(),
                    panel.left(),
                    "session panels share a left edge"
                );
                assert_eq!(
                    header.right(),
                    panel.right(),
                    "session panels share a right edge"
                );
                assert!(header.bottom() < panel.top(), "session panels have a gap");
                for selector in ["overlay-send-button", "overlay-effort-picker"] {
                    let button = visual
                        .debug_bounds(selector)
                        .expect("composer control laid out");
                    assert!(
                        button.origin.y >= panel.origin.y,
                        "{selector} begins above chat panel"
                    );
                    assert!(
                        button.origin.y + button.size.height <= panel.origin.y + panel.size.height,
                        "{selector} extends below chat panel: {button:?} vs {panel:?}"
                    );
                    assert!(
                        button.origin.y + button.size.height <= px(height),
                        "{selector} exceeds window height"
                    );
                }
            }
        }
    }
    assert!(!fixture.unused_data_dir.exists());
}

#[gpui_kit::test]
fn selected_hosted_media_controls_follow_committed_model_and_fail_closed_on_refresh_error(
    cx: &mut TestAppContext,
) {
    use skillissue_domain::{
        models::ProviderMode,
        provider::{ModelCapabilities, ProviderModel},
    };
    let fixture = Fixture::new(cx);
    fixture.update(cx, |view, _, _| {
        view.settings.provider_mode = ProviderMode::Palepale;
        view.settings.palepale.chat_model = "multimodal".into();
        view.settings.palepale.transcription_model = "speech".into();
        view.settings_ui.catalog_models = vec![
            ProviderModel {
                id: "multimodal".into(),
                capabilities: ModelCapabilities {
                    streaming: true,
                    image_input: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            ProviderModel {
                id: "speech".into(),
                capabilities: ModelCapabilities {
                    streaming: true,
                    audio_input: true,
                    ..Default::default()
                },
                ..Default::default()
            },
        ];
        assert!(view.supports_audio_input());
        assert!(view.supports_image_input());
        view.settings_ui.palepale_model = "uncommitted-draft".into();
        assert!(view.supports_image_input());
        view.settings_ui.catalog_loading = true;
        assert!(!view.supports_audio_input());
        assert!(!view.supports_image_input());
        view.settings_ui.catalog_loading = false;
        view.settings_ui.catalog_error = Some("Refresh failed".into());
        assert!(!view.supports_audio_input());
        view.settings_ui.catalog_error = None;
        view.settings.palepale.chat_model = "different-model".into();
        assert!(view.supports_audio_input());
        assert!(!view.supports_image_input());
        view.settings.palepale.transcription_model = "withdrawn-audio".into();
        assert!(!view.supports_audio_input());
        assert!(
            view.hosted_audio_unavailable_message()
                .contains("Palepale audio is unavailable")
        );
        view.settings.provider_mode = ProviderMode::Byok;
        assert!(view.supports_audio_input());
        assert!(view.supports_image_input());
    });
    assert!(!fixture.unused_data_dir.exists());
}

#[gpui_kit::test]
fn provider_label_stays_compact_across_readiness_and_media_changes(cx: &mut TestAppContext) {
    use skillissue_domain::{
        models::ProviderMode,
        provider::{ModelCapabilities, ProviderModel},
    };
    let fixture = Fixture::new(cx);
    fixture.update(cx, |view, _, _| {
        view.settings.provider_mode = ProviderMode::Palepale;
        assert_eq!(view.provider_label(), "Palepale");
        view.settings_ui.has_palepale_key = true;
        assert_eq!(view.provider_label(), "Palepale");
        view.settings_ui.catalog_loading = true;
        assert_eq!(view.provider_label(), "Palepale");
        assert!(
            view.hosted_audio_unavailable_message()
                .starts_with("Checking")
        );
        view.settings_ui.catalog_loading = false;
        view.settings_ui.catalog_error = Some("Synthetic failed refresh".into());
        assert_eq!(view.provider_label(), "Palepale");
        assert!(
            view.hosted_audio_unavailable_message()
                .contains("Refresh Palepale")
        );
        view.settings_ui.catalog_error = None;
        view.settings_ui.palepale_account_result =
            Some(Ok(skillissue_core::PalepaleAccountStatus {
                can_spend: true,
                ..Default::default()
            }));
        view.settings.palepale.chat_model = "hosted".into();
        view.settings.palepale.transcription_model = "hosted".into();
        view.settings_ui.catalog_models = vec![ProviderModel {
            id: "hosted".into(),
            capabilities: ModelCapabilities {
                streaming: true,
                ..Default::default()
            },
            ..Default::default()
        }];
        for (audio, image) in [(false, false), (true, false), (false, true), (true, true)] {
            view.settings_ui.catalog_models[0].capabilities.audio_input = audio;
            view.settings_ui.catalog_models[0].capabilities.image_input = image;
            assert_eq!(view.provider_label(), "Palepale");
        }
        view.settings_ui.palepale_account_result =
            Some(Ok(skillissue_core::PalepaleAccountStatus {
                can_spend: false,
                ..Default::default()
            }));
        assert_eq!(view.provider_label(), "Palepale");
        view.settings_ui.palepale_account_result =
            Some(Ok(skillissue_core::PalepaleAccountStatus {
                can_spend: true,
                ..Default::default()
            }));
        view.settings.palepale.chat_model = "withdrawn".into();
        assert_eq!(view.provider_label(), "Palepale");
        view.settings.provider_mode = ProviderMode::Byok;
        assert_eq!(view.provider_label(), "BYOK");
    });
    assert!(!fixture.unused_data_dir.exists());
}

#[gpui_kit::test]
fn empty_text_only_assist_focuses_input_without_starting_a_request(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    fixture.update(cx, |view, window, cx| {
        view.settings.provider_mode = skillissue_domain::models::ProviderMode::Palepale;
        view.settings_ui.catalog_models.clear();
        view.settings.include_screenshot = true;
        view.error = Some("Previous error".into());
        let generation = view.request_generation;
        view.ask(window, cx);
        assert!(view.typing);
        assert!(view.error.is_none());
        assert!(!view.busy);
        assert_eq!(view.request_generation, generation);
        assert!(view.request_cancel.is_none());
        assert_eq!(view.status, "Type a question to start a chat.");
    });
    assert!(!fixture.unused_data_dir.exists());
}

#[gpui_kit::test]
fn overlay_question_placeholder_tracks_typing_and_delete_without_parent_refresh(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx);
    fixture.update(cx, |view, window, cx| {
        view.page = Page::Live;
        view.review_id = "chat".into();
        view.gallery = true;
        view.expanded = false;
        view.overlay_ui.chat_shown = true;
        view.question.focus_handle(cx).focus(window, cx);
    });
    fixture.render(cx);
    assert!(
        gpui_kit::VisualTestContext::from_window(fixture.window, cx)
            .debug_bounds("overlay-question-placeholder")
            .is_none(),
        "focused empty composer must hide its shortcut placeholder"
    );

    cx.simulate_input(fixture.window, "draft");
    fixture.render(cx);
    assert!(
        gpui_kit::VisualTestContext::from_window(fixture.window, cx)
            .debug_bounds("overlay-question-placeholder")
            .is_none(),
        "typing must remove the parent-rendered placeholder"
    );

    cx.simulate_keystrokes(fixture.window, "ctrl-a backspace");
    fixture.render(cx);
    assert!(
        gpui_kit::VisualTestContext::from_window(fixture.window, cx)
            .debug_bounds("overlay-question-placeholder")
            .is_none(),
        "deleting while focused must not cover the caret with a placeholder"
    );
    fixture.update(cx, |view, window, cx| {
        view.root_focus.focus(window, cx);
    });
    fixture.render(cx);
    assert!(
        gpui_kit::VisualTestContext::from_window(fixture.window, cx)
            .debug_bounds("overlay-question-placeholder")
            .is_some(),
        "empty placeholder returns after leaving the field"
    );
    assert!(!fixture.unused_data_dir.exists());
}

#[gpui_kit::test]
fn overlay_errors_reserve_space_below_the_composer(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    for (width, height) in [(560., 550.), (800., 600.)] {
        cx.simulate_window_resize(fixture.window, size(px(width), px(height)));
        for theme in [ThemeMode::Light, ThemeMode::Dark] {
            for protected in [false, true] {
                fixture.update(cx, |view, window, cx| {
                    Theme::change(theme, Some(window), cx);
                    view.page = Page::Live;
                    view.review_id = "chat".into();
                    view.gallery = true;
                    view.settings.capture_protection = protected;
                    view.overlay_ui.chat_shown = true;
                    view.expanded = false;
                    view.error = Some(
                        "Palepale audio is unavailable. Use BYOK with a transcription provider. "
                            .repeat(20),
                    );
                    cx.notify();
                });
                fixture.render(cx);
                let mut visual = gpui_kit::VisualTestContext::from_window(fixture.window, cx);
                let panel = visual
                    .debug_bounds("overlay-chat-panel")
                    .expect("chat panel");
                let composer = visual.debug_bounds("overlay-composer").expect("composer");
                let error = visual
                    .debug_bounds("overlay-error-panel")
                    .expect("error panel");
                assert!(
                    error.origin.y >= composer.origin.y + composer.size.height,
                    "error overlaps composer: {error:?} vs {composer:?}"
                );
                assert!(
                    error.origin.y + error.size.height <= panel.origin.y + panel.size.height,
                    "error extends below chat panel"
                );
                for selector in [
                    "overlay-send-button",
                    "overlay-error-edit",
                    "overlay-error-dismiss",
                ] {
                    let button = visual.debug_bounds(selector).expect("visible action");
                    assert!(
                        button.origin.y + button.size.height <= panel.origin.y + panel.size.height,
                        "{selector} exceeds panel"
                    );
                    assert!(
                        button.origin.x + button.size.width <= panel.origin.x + panel.size.width,
                        "{selector} exceeds panel width"
                    );
                }
            }
        }
    }
    assert!(!fixture.unused_data_dir.exists());
}

#[gpui_kit::test]
fn transcription_progress_and_preparation_survive_window_updates(cx: &mut TestAppContext) {
    use skillissue_core::jobs::{TranscriptionProgress, TranscriptionState};
    let fixture = Fixture::new(cx);
    fixture.update(cx, |view, window, cx| {
        view.page = Page::Live;
        view.review_id = "chat".into();
        view.gallery = true;
        view.expanded = true;
        view.overlay_ui.chat_shown = true;
        view.overlay_ui.transcript = true;
        view.context.transcriptions = [
            TranscriptionState::Queued,
            TranscriptionState::Transcribing,
            TranscriptionState::Ready,
            TranscriptionState::NoSpeech,
            TranscriptionState::Failed,
        ]
        .into_iter()
        .enumerate()
        .map(|(id, state)| TranscriptionProgress {
            id: id as u64,
            source: "microphone".into(),
            created_at: 0,
            state,
        })
        .collect();
        for (kind, text) in [
            ("start", "What was agreed?"),
            ("preparing", "Waiting for recent speech…"),
        ] {
            view.on_event(
                skillissue_core::EngineEvent::Assistant {
                    id: "current".into(),
                    kind: kind.into(),
                    text: text.into(),
                },
                window,
                cx,
            );
        }
        assert!(view.audio_retry_prompt.is_none());
        view.on_event(
            skillissue_core::EngineEvent::Assistant {
                id: "old".into(),
                kind: "prepared".into(),
                text: String::new(),
            },
            window,
            cx,
        );
        assert!(view.overlay_ui.audio_notice.is_some());
        view.on_event(
            skillissue_core::EngineEvent::Assistant {
                id: "current".into(),
                kind: "cancelled".into(),
                text: String::new(),
            },
            window,
            cx,
        );
        assert!(view.overlay_ui.audio_notice.is_none());
        assert!(!view.busy);
    });
    fixture.render(cx);
    let mut visual = gpui_kit::VisualTestContext::from_window(fixture.window, cx);
    for selector in [
        "transcription-progress-0",
        "transcription-progress-1",
        "transcription-progress-2",
        "transcription-progress-3",
        "transcription-progress-4",
    ] {
        assert!(
            visual.debug_bounds(selector).is_some(),
            "{selector} is missing"
        );
    }
    assert!(!fixture.unused_data_dir.exists());
}

#[gpui_kit::test]
fn pending_audio_notice_survives_completion_and_clears_for_the_next_question(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx);
    fixture.update(cx, |view, window, cx| {
        view.page = Page::Live;
        view.review_id = "chat".into();
        view.gallery = true;
        view.overlay_ui.chat_shown = true;
        view.context.pending_transcriptions = 2;
        view.on_event(skillissue_core::EngineEvent::Assistant {
            id: "answer".into(), kind: "start".into(), text: "What was agreed?".into(),
        }, window, cx);
        view.on_event(skillissue_core::EngineEvent::Assistant {
            id: "answer".into(), kind: "context".into(),
            text: "Recent speech was still being transcribed when the audio wait ended. This answer may omit it; ask again after transcription finishes.".into(),
        }, window, cx);
        view.on_event(skillissue_core::EngineEvent::Assistant {
            id: "answer".into(), kind: "done".into(), text: String::new(),
        }, window, cx);
        assert!(view.overlay_ui.audio_notice.is_some());
    });
    fixture.render(cx);
    let mut visual = gpui_kit::VisualTestContext::from_window(fixture.window, cx);
    let notice = visual
        .debug_bounds("overlay-audio-notice")
        .expect("audio notice");
    let pending = visual
        .debug_bounds("overlay-audio-pending")
        .expect("pending audio");
    let composer = visual.debug_bounds("overlay-composer").expect("composer");
    assert!(pending.origin.y >= notice.origin.y + notice.size.height);
    assert!(composer.origin.y >= pending.origin.y + pending.size.height);
    let panel = visual.debug_bounds("overlay-chat-panel").expect("panel");
    assert!(composer.origin.y + composer.size.height <= panel.origin.y + panel.size.height);
    let retry = visual
        .debug_bounds("overlay-retry-completed-audio")
        .expect("retry action");
    visual.simulate_click(retry.center(), Default::default());
    fixture.update(cx, |view, window, cx| {
        assert_eq!(view.audio_retry_prompt.as_deref(), Some("What was agreed?"));
        assert!(view.question.read(cx).value().is_empty());
        view.question.update(cx, |input, cx| {
            input.set_value("My new question", window, cx)
        });
        view.retry_with_completed_audio(window, cx);
        assert_eq!(view.question.read(cx).value().as_str(), "My new question");
        assert_eq!(view.audio_retry_prompt.as_deref(), Some("What was agreed?"));
        view.context.pending_transcriptions = 0;
        view.question
            .update(cx, |input, cx| input.set_value("", window, cx));
        // An unavailable engine cannot consume a retry either.
        view.retry_with_completed_audio(window, cx);
        assert_eq!(view.audio_retry_prompt.as_deref(), Some("What was agreed?"));
        assert!(view.question.read(cx).value().is_empty());
        view.snapshot.session_id = None;
        let mut persisted = view.snapshot.clone();
        persisted.session_id = Some("first-session".into());
        view.apply_session_snapshot(persisted);
        assert_eq!(view.audio_retry_prompt.as_deref(), Some("What was agreed?"));
    });
    fixture.update(cx, |view, window, cx| {
        view.on_event(
            skillissue_core::EngineEvent::Assistant {
                id: "next".into(),
                kind: "start".into(),
                text: String::new(),
            },
            window,
            cx,
        );
        assert!(view.overlay_ui.audio_notice.is_none());
        assert!(view.audio_retry_prompt.is_none());
        view.overlay_ui.audio_notice = Some("old session warning".into());
        view.snapshot.session_id = Some("previous".into());
        let mut next = view.snapshot.clone();
        next.session_id = Some("next-session".into());
        view.apply_session_snapshot(next);
        assert!(view.overlay_ui.audio_notice.is_none());
    });
}
