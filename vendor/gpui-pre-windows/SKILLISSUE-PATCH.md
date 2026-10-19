# Windows glass shadow correction

Source: crates.io `gpui-pre-windows` 0.3.9, Zed snapshot93d745668805029476e6d7fbb19652116fed51de. Upstream Apache-2.0 license is preserved in LICENSE-APACHE.

The only renderer modification is src/shaders.hlsl: shadow_fragment clips outer shadows to the exterior of the original rounded border box. Inset shadows retain upstream behavior. CSS box shadows exclude that interior; upstream GPUI paints the full shadow behind the fill, visibly darkening translucent Liquid Glass surfaces. This patch aligns native compositing with the skillissue Liquid Glass design.

Debug builds compile the embedded HLSL at runtime; release builds compile the same HLSL through the upstream Windows SDK build script. No JavaScript, CSS or browser engine is involved.
