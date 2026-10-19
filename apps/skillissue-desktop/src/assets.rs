//! Embedded product assets.
use gpui_kit::{AssetSource, Result, SharedString};
use std::borrow::Cow;

const BRAND_ASSETS: &[(&str, &[u8])] = &[
    (
        "logo.png",
        include_bytes!("../../../assets/branding/logo.png"),
    ),
    (
        "logo.svg",
        include_bytes!("../../../assets/branding/logo.svg"),
    ),
];

pub struct Assets;
impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, bytes)) = crate::heroicons::ICONS
            .iter()
            .find(|(name, _)| *name == path)
        {
            return Ok(Some(Cow::Borrowed(*bytes)));
        }
        if let Some(name) = path.strip_prefix("brand/") {
            return Ok(BRAND_ASSETS
                .iter()
                .find(|(path, _)| *path == name)
                .map(|(_, bytes)| Cow::Borrowed(*bytes)));
        }
        match path {
            "workspace-light.svg" => {
                return Ok(Some(Cow::Borrowed(include_bytes!(
                    "../assets/workspace-light.svg"
                ))));
            }
            "workspace-dark.svg" => {
                return Ok(Some(Cow::Borrowed(include_bytes!(
                    "../assets/workspace-dark.svg"
                ))));
            }
            _ => {}
        }
        gpui_kit::assets::AllAssets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        if path.starts_with("brand/") {
            return Ok(BRAND_ASSETS
                .iter()
                .map(|(name, _)| SharedString::from(format!("brand/{name}")))
                .collect());
        }
        let mut assets: Vec<_> = ["workspace-light.svg", "workspace-dark.svg"]
            .into_iter()
            .filter(|name| name.starts_with(path))
            .map(SharedString::from)
            .collect();
        assets.extend(gpui_kit::assets::AllAssets.list(path)?);
        Ok(assets)
    }
}

pub fn load_fonts(cx: &gpui_kit::App) {
    if let Err(error) = cx.text_system().add_fonts(vec![
        Cow::Borrowed(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf")),
    ]) {
        eprintln!("Load interface fonts: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heroicons_override_component_icon_paths() {
        for (path, expected) in crate::heroicons::ICONS {
            let actual = Assets.load(path).unwrap().unwrap();
            assert_eq!(actual.as_ref(), *expected, "{path}");
            assert!(
                std::str::from_utf8(&actual)
                    .unwrap()
                    .contains("viewBox=\"0 0 24 24\"")
            );
        }
    }

    #[test]
    fn product_brand_is_compiled_into_the_binary_and_exposes_only_runtime_assets() {
        let png = Assets.load("brand/logo.png").unwrap().unwrap();
        assert!(matches!(png, Cow::Borrowed(_)));
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        let svg = Assets.load("brand/logo.svg").unwrap().unwrap();
        assert!(std::str::from_utf8(&svg).unwrap().contains("skillissue"));
        assert!(Assets.load("brand/preview-themes.png").unwrap().is_none());
        assert_eq!(Assets.list("brand/").unwrap().len(), 2);
    }
}
