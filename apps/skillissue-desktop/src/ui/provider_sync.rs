//! Non-secret provider readiness shared by the dashboard and its floating overlay.
use super::*;

#[derive(Clone)]
pub(super) struct ProviderStatusSnapshot {
    mode: ProviderMode,
    model: String,
    audio_model: String,
    has_palepale_key: bool,
    catalog_models: Vec<ProviderModel>,
    catalog_loading: bool,
    catalog_error: Option<String>,
    account_loading: bool,
    account_result: Option<Result<skillissue_core::PalepaleAccountStatus, String>>,
}

impl NativeView {
    pub(super) fn invalidate_hosted_provider_status(&mut self, mode_changed: bool) {
        self.cancel_palepale_checks();
        // A completed catalog response may itself commit a replacement model. Keep that
        // response, but discard a previous window's catalog that cannot resolve the target.
        if mode_changed
            || !self.settings_ui.catalog_models.iter().any(|model| {
                model.id == self.settings.palepale.chat_model && model.capabilities.streaming
            })
        {
            self.settings_ui.catalog_models.clear();
            self.settings_ui.catalog_error = None;
        }
    }

    pub(super) fn provider_status_snapshot(&self) -> ProviderStatusSnapshot {
        ProviderStatusSnapshot {
            mode: self.settings.provider_mode,
            model: self.settings.palepale.chat_model.clone(),
            audio_model: self.settings.palepale.transcription_model.clone(),
            has_palepale_key: self.settings_ui.has_palepale_key,
            catalog_models: self.settings_ui.catalog_models.clone(),
            catalog_loading: self.settings_ui.catalog_loading,
            catalog_error: self.settings_ui.catalog_error.clone(),
            account_loading: self.settings_ui.palepale_account_loading,
            account_result: self.settings_ui.palepale_account_result.clone(),
        }
    }

    pub(super) fn apply_provider_status_snapshot(&mut self, status: &ProviderStatusSnapshot) {
        if self.settings.provider_mode != status.mode
            || self.settings.palepale.chat_model != status.model
            || self.settings.palepale.transcription_model != status.audio_model
            || self.settings_ui.has_palepale_key != status.has_palepale_key
        {
            return;
        }
        // Invalidate independent requests before copying the dashboard's state. Their
        // queued replies must not restore a stale catalog or a previous account result.
        self.cancel_palepale_checks();
        self.settings_ui.catalog_models = status.catalog_models.clone();
        self.settings_ui.catalog_loading = status.catalog_loading;
        self.settings_ui.catalog_error = status.catalog_error.clone();
        self.settings_ui.palepale_account_loading = status.account_loading;
        self.settings_ui.palepale_account_result = status.account_result.clone();
    }

    pub(super) fn sync_provider_status_to_overlay(&self, cx: &mut Context<Self>) {
        if let Some((_, overlay)) = &self.overlay {
            let status = self.provider_status_snapshot();
            overlay.update(cx, |overlay, cx| {
                overlay.apply_provider_status_snapshot(&status);
                cx.notify();
            });
        }
    }

    pub(super) fn sync_provider_status_from_parent(&mut self, cx: &mut Context<Self>) {
        let status = self
            .parent
            .as_ref()
            .and_then(|(_, parent)| parent.upgrade())
            .map(|parent| parent.read(cx).provider_status_snapshot());
        if let Some(status) = status {
            self.apply_provider_status_snapshot(&status);
        }
    }

    pub(super) fn expand_overlay_for_error(&mut self, window: &mut Window) -> bool {
        let audio_status = self.overlay_ui.chat_shown
            && (self.overlay_ui.audio_notice.is_some() || self.context.pending_transcriptions > 0);
        if self.is_overlay && (self.error.is_some() || audio_status) && !self.expanded {
            self.expanded = true;
            window.resize(size(px(560.), px(550.)));
            return true;
        }
        false
    }
}

#[cfg(all(test, feature = "native-effects"))]
mod tests {
    use super::NativeView;
    use gpui_kit::{
        AnyWindowHandle, AppContext, Bounds, Entity, TestAppContext, WindowBounds, WindowOptions,
        component::Theme, point, px, size,
    };
    use skillissue_domain::{
        models::{ConfigView, ProviderMode},
        provider::ProviderModel,
    };
    use std::sync::Arc;

    fn test_view(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<NativeView>) {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("inert test runtime"),
        );
        cx.update(|cx| {
            if !cx.has_global::<Theme>() {
                gpui_kit::init(cx);
            }
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: point(px(0.), px(0.)),
                        size: size(px(560.), px(220.)),
                    })),
                    focus: false,
                    titlebar: None,
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    cx.new(|cx| {
                        let mut view = NativeView::new_for_test(
                            runtime,
                            std::env::temp_dir()
                                .join(format!("skillissue-provider-sync-{}", uuid::Uuid::new_v4())),
                            window,
                            cx,
                        );
                        view.gallery = true;
                        view.settings.provider_mode = ProviderMode::Palepale;
                        view.settings.palepale.chat_model = "old".into();
                        view.settings_ui.has_palepale_key = true;
                        view
                    })
                },
            )
            .expect("mock provider window")
        })
    }

    fn model(id: &str, image_input: bool) -> ProviderModel {
        ProviderModel {
            id: id.into(),
            capabilities: skillissue_domain::provider::ModelCapabilities {
                streaming: true,
                image_input,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[gpui_kit::test]
    fn changed_hosted_target_discards_nonempty_stale_catalog(cx: &mut TestAppContext) {
        let (handle, view) = test_view(cx);
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.settings_ui.catalog_models = vec![model("old", true)];
                view.settings_ui.catalog_error = Some("old failure".into());
                view.settings_ui.palepale_account_result =
                    Some(Ok(skillissue_core::PalepaleAccountStatus {
                        can_spend: true,
                        ..Default::default()
                    }));
                let mut settings = view.settings.clone();
                settings.palepale.chat_model = "new".into();
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
                assert!(view.settings_ui.catalog_models.is_empty());
                assert!(view.settings_ui.catalog_error.is_none());
                assert!(view.settings_ui.palepale_account_result.is_none());
                assert!(view.selected_hosted_model().is_none());
            });
        })
        .expect("apply committed model change");
    }

    #[gpui_kit::test]
    fn dashboard_refresh_updates_overlay_and_rejects_old_child_replies(cx: &mut TestAppContext) {
        let (parent_handle, parent) = test_view(cx);
        let (child_handle, child) = test_view(cx);
        cx.update(|cx| {
            child.update(cx, |child, _| {
                child.is_overlay = true;
                child.parent = Some((parent_handle, parent.downgrade()));
                child.settings_ui.catalog_models = vec![model("old", false)];
                child.settings_ui.catalog_loading = true;
                child.settings_ui.catalog_generation = 11;
                child.settings_ui.palepale_account_loading = true;
                child.settings_ui.palepale_account_generation = 12;
                child.settings_ui.catalog_cancel = Some(tokio_util::sync::CancellationToken::new());
                child.settings_ui.palepale_account_cancel =
                    Some(tokio_util::sync::CancellationToken::new());
            });
            parent.update(cx, |parent, cx| {
                parent.overlay = Some((child_handle, child.clone()));
                parent.settings_ui.catalog_models = vec![model("old", true)];
                parent.settings_ui.palepale_account_result =
                    Some(Ok(skillissue_core::PalepaleAccountStatus {
                        can_spend: true,
                        ..Default::default()
                    }));
                parent.sync_provider_status_to_overlay(cx);
            });
            child.update(cx, |child, cx| {
                child.sync_provider_status_from_parent(cx);
                assert!(child.supports_image_input());
                assert_eq!(child.palepale_connection_status().0, "Palepale");
                child.receive_provider_catalog(11, Err("obsolete child error".into()));
                child.receive_palepale_account(12, Err("obsolete account error".into()));
                assert!(child.supports_image_input());
                assert_eq!(child.palepale_connection_status().0, "Palepale");
                assert!(child.settings_ui.catalog_cancel.is_none());
                assert!(child.settings_ui.palepale_account_cancel.is_none());
            });
            parent.update(cx, |parent, cx| {
                parent.settings_ui.catalog_models = vec![model("old", false)];
                parent.settings_ui.catalog_error = Some("new refresh failed".into());
                parent.sync_provider_status_to_overlay(cx);
            });
            child.update(cx, |child, _| {
                assert!(!child.supports_image_input());
                assert_eq!(child.palepale_connection_status().0, "Palepale unavailable");
            });
        });
    }

    #[gpui_kit::test]
    fn mismatched_parent_target_cannot_restore_old_readiness(cx: &mut TestAppContext) {
        let (_, parent) = test_view(cx);
        let (_, child) = test_view(cx);
        cx.update(|cx| {
            let status = parent.read(cx).provider_status_snapshot();
            child.update(cx, |child, _| {
                child.settings.palepale.chat_model = "new".into();
                child.settings_ui.catalog_models = vec![model("new", true)];
                child.apply_provider_status_snapshot(&status);
                assert_eq!(child.settings_ui.catalog_models[0].id, "new");
                assert!(child.supports_image_input());
            });
        });
    }

    #[gpui_kit::test]
    fn mismatched_audio_target_cannot_restore_old_readiness(cx: &mut TestAppContext) {
        let (_, parent) = test_view(cx);
        let (_, child) = test_view(cx);
        cx.update(|cx| {
            let status = parent.read(cx).provider_status_snapshot();
            child.update(cx, |child, _| {
                child.settings.palepale.transcription_model = "new-audio".into();
                child.settings_ui.catalog_error = Some("new audio refresh required".into());
                child.apply_provider_status_snapshot(&status);
                assert_eq!(
                    child.settings_ui.catalog_error.as_deref(),
                    Some("new audio refresh required")
                );
            });
        });
    }

    #[gpui_kit::test]
    fn collapsed_overlay_expands_once_for_an_error(cx: &mut TestAppContext) {
        let (handle, view) = test_view(cx);
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, _| {
                view.is_overlay = true;
                view.expanded = false;
                view.error = Some("Audio is unavailable".into());
                assert!(view.expand_overlay_for_error(window));
                assert!(view.expanded);
                assert!(!view.expand_overlay_for_error(window));
                assert_eq!(view.error.as_deref(), Some("Audio is unavailable"));
            });
        })
        .expect("expand mock overlay for its error");
    }
}
