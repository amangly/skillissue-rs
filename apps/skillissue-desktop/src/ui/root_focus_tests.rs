// Rust | ui/root_focus_tests.rs | GPUI TestPlatform | startup and removed-child keyboard routing
use super::{NativeView, Page};
use gpui_kit::{
    AnyWindowHandle, AppContext, Bounds, Entity, Focusable, InputEvent as _, KeyDownEvent,
    KeyUpEvent, Keystroke, Modifiers, TestAppContext, VisualTestContext, WindowBounds,
    WindowOptions, component::Theme, point, px, size,
};
use std::{path::PathBuf, sync::Arc};

struct Fixture {
    handle: AnyWindowHandle,
    view: Entity<NativeView>,
    unused_directory: PathBuf,
}

impl Fixture {
    fn new(cx: &mut TestAppContext) -> Self {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("inert focus test runtime"),
        );
        let unused_directory =
            std::env::temp_dir().join(format!("skillissue-root-focus-{}", uuid::Uuid::new_v4()));
        let (handle, view) = cx.update(|cx| {
            if !cx.has_global::<Theme>() {
                gpui_kit::init(cx);
            }
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: point(px(0.), px(0.)),
                        size: size(px(1100.), px(800.)),
                    })),
                    focus: false,
                    titlebar: None,
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    cx.new(|cx| {
                        let mut view =
                            NativeView::new_for_test(runtime, unused_directory.clone(), window, cx);
                        view.gallery = false;
                        view.page = Page::Sessions;
                        view
                    })
                },
            )
            .expect("inert root focus window")
        });
        let fixture = Self {
            handle,
            view,
            unused_directory,
        };
        fixture.draw(cx);
        fixture
    }

    fn draw(&self, cx: &mut TestAppContext) {
        cx.update_window(self.handle, |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
            window.simulate_next_frame(cx);
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .expect("draw focus lifecycle");
        cx.run_until_parked();
    }

    fn press(&self, key: &str, cx: &mut TestAppContext) {
        let keystroke = Keystroke::parse(key).unwrap();
        cx.update_window(self.handle, |_, window, cx| {
            window.dispatch_event(
                KeyDownEvent {
                    keystroke: keystroke.clone(),
                    is_held: false,
                    prefer_character_input: false,
                }
                .to_platform_input(),
                cx,
            );
            window.dispatch_event(KeyUpEvent { keystroke }.to_platform_input(), cx);
        })
        .expect("dispatch real render-tree key");
        self.draw(cx);
    }

    fn visible(&self, selector: &'static str, cx: &mut TestAppContext) -> bool {
        VisualTestContext::from_window(self.handle, cx)
            .debug_bounds(selector)
            .is_some()
    }

    fn assert_inert(&self, cx: &TestAppContext) {
        self.view.read_with(cx, |view, _| {
            assert!(view.engine.is_none());
            assert!(view.native.is_none());
            assert!(view.hotkeys.is_none());
        });
        assert!(!self.unused_directory.exists());
    }
}

#[gpui_kit::test]
fn fresh_dashboard_routes_ctrl_k_without_focusing_an_input_or_tab_stop(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    cx.update_window(fixture.handle, |_, window, cx| {
        fixture.view.update(cx, |view, _| {
            assert!(view.root_focus.is_focused(window));
        });
    })
    .unwrap();
    fixture.press("ctrl-k", cx);
    fixture
        .view
        .read_with(cx, |view, _| assert!(view.dashboard_modal_open()));
    fixture.press("escape", cx);
    fixture
        .view
        .read_with(cx, |view, _| assert!(!view.dashboard_modal_open()));
    fixture.assert_inert(cx);
}

#[gpui_kit::test]
fn fresh_dashboard_menu_mouse_click_routes_escape_without_prior_input_focus(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx);
    {
        let mut visual = VisualTestContext::from_window(fixture.handle, cx);
        let bounds = visual
            .debug_bounds("dashboard-menu")
            .expect("dashboard menu button");
        visual.simulate_click(bounds.center(), Modifiers::default());
    }
    fixture.draw(cx);
    assert!(fixture.visible("workspace-settings", cx));
    fixture.press("escape", cx);
    assert!(!fixture.visible("workspace-settings", cx));
    fixture.assert_inert(cx);
}

#[gpui_kit::test]
fn removing_a_focused_composer_restores_root_routing_without_stealing_live_input_focus(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx);
    cx.update_window(fixture.handle, |_, _, cx| {
        fixture.view.update(cx, |view, cx| {
            view.page = Page::Live;
            cx.notify();
        });
    })
    .unwrap();
    fixture.draw(cx);
    cx.update_window(fixture.handle, |_, window, cx| {
        fixture.view.update(cx, |view, cx| {
            view.question.focus_handle(cx).focus(window, cx)
        });
    })
    .unwrap();
    fixture.draw(cx);
    cx.update_window(fixture.handle, |_, window, cx| {
        fixture.view.update(cx, |view, cx| {
            assert!(view.question.focus_handle(cx).is_focused(window));
            view.page = Page::Sessions;
            cx.notify();
        });
    })
    .unwrap();
    fixture.draw(cx);
    cx.update_window(fixture.handle, |_, window, cx| {
        fixture
            .view
            .update(cx, |view, _| assert!(view.root_focus.is_focused(window)));
    })
    .unwrap();
    fixture.press("ctrl-k", cx);
    fixture
        .view
        .read_with(cx, |view, _| assert!(view.dashboard_modal_open()));
    fixture.assert_inert(cx);
}

#[gpui_kit::test]
fn explicit_blur_recovers_root_keyboard_routing_on_the_next_frame(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx);
    cx.update_window(fixture.handle, |_, window, cx| window.blur(cx))
        .unwrap();
    fixture.draw(cx);
    fixture.press("ctrl-k", cx);
    fixture
        .view
        .read_with(cx, |view, _| assert!(view.dashboard_modal_open()));
    fixture.assert_inert(cx);
}
