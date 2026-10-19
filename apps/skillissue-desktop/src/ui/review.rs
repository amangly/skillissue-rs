// Rust | ui/review.rs | GPUI ui-review feature | reproducible native render evidence
use super::*;

pub(super) struct ReviewRun {
    output: PathBuf,
    captured: Vec<String>,
    errors: Vec<String>,
    next: usize,
    background_ready: bool,
}

impl ReviewRun {
    pub(super) fn from_args() -> Option<Self> {
        let output = std::env::args()
            .find_map(|arg| arg.strip_prefix("--review-output=").map(PathBuf::from))?;
        Some(Self {
            output,
            captured: vec![],
            errors: vec![],
            // Capture the initial dashboard after the other routes have warmed
            // native SVG/image caches, then finish the product screen catalog.
            next: 1,
            background_ready: false,
        })
    }
}

impl NativeView {
    pub(super) fn poll_review(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.engine.is_none()
            || self.ticks < 24
            || !self.ticks.is_multiple_of(8)
            || self.capturing_backdrop
        {
            return false;
        }
        let Some(mut run) = self.review.take() else {
            return false;
        };
        // Keep review scenes deterministic regardless of any saved profile connection.
        self.has_key = false;
        self.settings_ui.has_byok_key = false;
        self.settings_ui.has_palepale_key = false;
        self.settings_ui.palepale_pairing = settings::PalepalePairingState::Idle;
        self.cancel_palepale_account();
        if self.settings.is_palepale()
            && self.settings.palepale.chat_model == "preview-model"
            && !self.settings_ui.catalog_models.iter().any(|model| {
                model.id == "preview-model"
                    && model.capabilities.image_input
                    && model.capabilities.audio_input
            })
        {
            self.settings.palepale.transcription_model = "preview-model".into();
            if let Some(cancel) = self.settings_ui.catalog_cancel.take() {
                cancel.cancel();
            }
            self.settings_ui.catalog_generation += 1;
            self.settings_ui.catalog_loading = false;
            self.settings_ui.catalog_error = None;
            self.settings_ui.catalog_models = vec![ProviderModel {
                id: "preview-model".into(),
                display_name: "Preview multimodal model".into(),
                capabilities: skillissue_domain::provider::ModelCapabilities {
                    streaming: true,
                    reasoning: true,
                    image_input: true,
                    audio_input: true,
                    ..Default::default()
                },
                reasoning_efforts: vec!["low".into(), "medium".into(), "high".into()],
                default_reasoning_effort: Some("low".into()),
            }];
        }
        if !run.background_ready {
            let ImageSource::Resource(resource) = ImageSource::from(if self.is_dark(cx) {
                "workspace-dark.svg"
            } else {
                "workspace-light.svg"
            }) else {
                unreachable!()
            };
            match window.get_asset::<ImgResourceLoader>(&resource, cx) {
                Some(Ok(_)) => run.background_ready = true,
                Some(Err(error)) => {
                    eprintln!("Review background could not load: {error}");
                    cx.quit();
                    return true;
                }
                None => {}
            }
            self.review = Some(run);
            window.refresh();
            cx.notify();
            return true;
        }
        let expected = &catalog::REVIEW_SCREENS[run.next];
        if self.review_id != expected.id {
            self.select_review(expected.id, window, cx);
            self.review = Some(run);
            return true;
        }
        let path = run.output.join(format!("{}.png", expected.id));
        let result = (|| -> anyhow::Result<()> {
            std::fs::create_dir_all(&run.output)?;
            if path.exists() {
                anyhow::bail!(
                    "Refusing to replace existing review image {}",
                    path.display()
                );
            }
            window.render_to_image()?.save(&path)?;
            Ok(())
        })();
        match result {
            Ok(()) => run.captured.push(expected.id.into()),
            Err(error) => run.errors.push(format!("{}: {error:#}", expected.id)),
        }
        run.next = (run.next + 1) % catalog::REVIEW_SCREENS.len();
        if run.captured.len() + run.errors.len() == catalog::REVIEW_SCREENS.len() {
            let report = serde_json::json!({
                "renderer":"GPUI native DirectX scene readback",
                "viewport":[f32::from(window.viewport_size().width), f32::from(window.viewport_size().height)],
                "scaleFactor":window.scale_factor(),
                "pixelDimensions":[(f32::from(window.viewport_size().width) * window.scale_factor()) as u32, (f32::from(window.viewport_size().height) * window.scale_factor()) as u32],
                "providerMode":self.settings.provider_mode,
                "theme":self.settings.theme, "expectedScreens":catalog::REVIEW_SCREENS.len(), "screens":run.captured, "errors":run.errors,
                "scope":"Rendered native frames; interaction, speech and receiver capture are separate checks.",
                "accountEvidence":"Synthetic disconnected fixture; host credentials ignored, no account request sent",
                "catalogueEvidence":if self.settings.is_palepale() && self.settings.palepale.chat_model == "preview-model" {
                    "Synthetic enabled-media fixture, not supplier verification"
                } else { "Current UI metadata" }
            });
            if let Err(error) =
                std::fs::write(run.output.join("render-report.json"), report.to_string())
            {
                eprintln!("Write native review report: {error}");
            }
            cx.quit();
        } else {
            self.select_review(catalog::REVIEW_SCREENS[run.next].id, window, cx);
            self.review = Some(run);
        }
        true
    }
}
