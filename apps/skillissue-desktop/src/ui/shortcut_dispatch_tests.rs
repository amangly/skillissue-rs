// Rust | ui/shortcut_dispatch_tests.rs | GPUI TestPlatform | global shortcut routing
use super::{NativeView, Page};
use gpui_kit::{
    AppContext, Context, Entity, TestAppContext, Window, WindowOptions,
    component::{Theme, ThemeMode},
};
use skillissue_platform::shortcuts::Action;
use std::sync::Arc;

fn with_view(
    cx: &mut TestAppContext,
    test: impl FnOnce(&mut NativeView, &mut Window, &mut Context<NativeView>),
) {
    let runtime = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("inert test runtime"),
    );
    let data_dir =
        std::env::temp_dir().join(format!("skillissue-shortcut-{}", uuid::Uuid::new_v4()));
    let (window, view): (_, Entity<NativeView>) = cx.update(|cx| {
        if !cx.has_global::<Theme>() {
            gpui_kit::init(cx);
        }
        Theme::change(ThemeMode::Light, None, cx);
        gpui_kit::open_window(
            WindowOptions {
                focus: false,
                titlebar: None,
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
                    let mut view = NativeView::new_for_test(runtime, data_dir.clone(), window, cx);
                    view.gallery = false;
                    view.page = Page::Live;
                    view.settings.include_screenshot = false;
                    view
                })
            },
        )
        .expect("mock shortcut window")
    });
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| test(view, window, cx));
    })
    .expect("dispatch in mock window");
    assert!(!data_dir.exists(), "shortcut tests must not open a profile");
}

fn attach_overlay(parent: &mut NativeView, cx: &mut Context<NativeView>) -> Entity<NativeView> {
    let runtime = parent.runtime.clone();
    let data_dir = parent.data_dir.clone();
    let pair = gpui_kit::open_window(
        WindowOptions {
            focus: false,
            titlebar: None,
            ..Default::default()
        },
        cx,
        |window, cx| {
            cx.new(|cx| {
                let mut view = NativeView::new_for_test(runtime, data_dir, window, cx);
                view.gallery = false;
                view.is_overlay = true;
                view.shown = false;
                view.overlay_ui.chat_shown = false;
                view.settings.include_screenshot = false;
                view
            })
        },
    )
    .expect("mock existing overlay");
    let view = pair.1.clone();
    parent.overlay = Some(pair);
    view
}

#[gpui_kit::test]
fn clear_shortcut_does_not_create_an_overlay(cx: &mut TestAppContext) {
    with_view(cx, |view, window, cx| {
        view.shown = false;
        view.stream = "Partial answer".into();
        view.dispatch(Action::Clear, window, cx);
        assert!(view.overlay.is_none());
        assert!(!view.shown);
        assert!(view.stream.is_empty());
    });
}

#[gpui_kit::test]
fn empty_global_assist_reveals_hidden_chat_and_focuses_the_composer(cx: &mut TestAppContext) {
    with_view(cx, |view, window, cx| {
        view.is_overlay = true;
        view.shown = false;
        view.overlay_ui.chat_shown = false;
        view.dispatch(Action::Ask, window, cx);
        assert!(view.shown);
        assert!(view.overlay_ui.chat_shown);
        assert!(view.typing);
        assert!(!view.busy);
        assert!(view.error.is_none());
        assert_eq!(view.status, "Type a question to start a chat.");
    });
}

#[gpui_kit::test]
fn global_assist_reveals_collapsed_chat_before_submitting(cx: &mut TestAppContext) {
    with_view(cx, |view, window, cx| {
        view.is_overlay = true;
        view.shown = false;
        view.overlay_ui.chat_shown = false;
        view.question
            .update(cx, |input, cx| input.set_value("A local draft", window, cx));
        view.dispatch(Action::Ask, window, cx);
        assert!(view.shown);
        assert!(view.overlay_ui.chat_shown);
        assert!(view.expanded);
        assert!(!view.busy, "the fixture has no engine or inference client");
    });
}

#[gpui_kit::test]
fn movement_recovers_a_hidden_widget_without_expanding_chat(cx: &mut TestAppContext) {
    with_view(cx, |view, window, cx| {
        view.is_overlay = true;
        view.expanded = false;
        view.shown = false;
        view.overlay_ui.chat_shown = false;
        view.dispatch(Action::MoveLeft, window, cx);
        assert!(view.shown);
        assert!(!view.overlay_ui.chat_shown);
        assert!(!view.expanded);
        assert!(!view.typing);
    });
}

#[gpui_kit::test]
fn scrolling_recovers_hidden_chat_without_starting_assist(cx: &mut TestAppContext) {
    with_view(cx, |view, window, cx| {
        view.is_overlay = true;
        view.shown = false;
        view.overlay_ui.chat_shown = false;
        view.dispatch(Action::ScrollDown, window, cx);
        assert!(view.shown);
        assert!(view.overlay_ui.chat_shown);
        assert!(!view.busy);
        assert!(!view.typing);
    });
}

#[gpui_kit::test]
fn visibility_shortcut_restores_a_widget_after_hide_widget_hides_it(cx: &mut TestAppContext) {
    with_view(cx, |view, window, cx| {
        view.is_overlay = true;
        view.settings.hide_widget = true;
        view.shown = true;
        view.overlay_ui.chat_shown = true;
        view.dispatch(Action::ToggleVisibility, window, cx);
        assert!(!view.shown);
        assert!(!view.overlay_ui.chat_shown);
        view.dispatch(Action::ToggleVisibility, window, cx);
        assert!(view.shown);
        assert!(view.overlay_ui.chat_shown);
    });
}

#[gpui_kit::test]
fn hidden_preview_recovers_even_when_shown_flag_is_stale(cx: &mut TestAppContext) {
    with_view(cx, |view, window, cx| {
        view.is_overlay = true;
        view.shown = true;
        view.preview_hidden = true;
        view.overlay_ui.chat_shown = false;
        view.dispatch(Action::ToggleVisibility, window, cx);
        assert!(view.shown);
        assert!(!view.preview_hidden);
        assert!(view.overlay_ui.chat_shown);
    });
}

#[gpui_kit::test]
fn settings_shortcut_opens_settings_without_creating_an_overlay(cx: &mut TestAppContext) {
    with_view(cx, |view, window, cx| {
        view.dispatch(Action::Settings, window, cx);
        assert!(view.page == Page::Settings);
        assert!(view.overlay.is_none());
    });
}

#[gpui_kit::test]
fn parent_routes_global_assist_to_the_existing_hidden_overlay(cx: &mut TestAppContext) {
    with_view(cx, |view, window, cx| {
        let overlay = attach_overlay(view, cx);
        view.dispatch(Action::Ask, window, cx);
        let overlay = overlay.read(cx);
        assert!(overlay.shown);
        assert!(overlay.overlay_ui.chat_shown);
        assert!(overlay.typing);
        assert!(!view.typing);
        assert!(!view.busy);
        assert!(!overlay.busy);
    });
}

#[gpui_kit::test]
fn parent_routes_clear_to_existing_overlay_and_keeps_it_hidden(cx: &mut TestAppContext) {
    with_view(cx, |view, window, cx| {
        let overlay = attach_overlay(view, cx);
        overlay.update(cx, |overlay, _| {
            overlay.stream = "Overlay partial answer".into()
        });
        view.stream = "Dashboard draft".into();
        view.dispatch(Action::Clear, window, cx);
        let overlay = overlay.read(cx);
        assert!(overlay.stream.is_empty());
        assert!(!overlay.shown);
        assert!(!overlay.overlay_ui.chat_shown);
        assert_eq!(view.stream, "Dashboard draft");
    });
}
