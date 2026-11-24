# HQ app icon

A watchful teal owl with an ivory face and orange beak and feet, on a charcoal background.
The artwork was generated with the built-in imagegen tool using Finch's flattened icon as a style reference.
The original generated foreground is preserved in `AppIcon.icon/Assets/owl.png` with alpha transparency.
`AppIcon.icon/icon.json` defines its layout and background in Icon Composer.

The iPhone target compiles this document under the AppIcon name, including fallback images for older supported iOS versions.
Run `script/make_icon.swift` to export the Mac PNG, icon sizes, and ICNS from the same document.
The Mac wrapper bundles `macos/Assets/AppIcon.icns`.

## Generation prompt

```text
Use case: logo-brand. Asset type: foreground artwork for the HQ iPhone and Mac app icon, 1024 x 1024 square transparent PNG. Reference image 1 is ONLY the visual style reference, Finch's bird icon. Create ONE new icon character, a small watchful owl for HQ, the place to review commitments and decide what needs attention. Match the reference's polished gently sculpted satin illustration, restrained soft shading, bold readable shapes and friendly alert expression. The owl faces forward with a slight three-quarter turn, two large dark eyes inside a simple warm ivory facial disc, small orange beak, a neat compact blue-teal body, darker folded wings, modest ear tufts, and two small feet. Sophisticated, calm and appealing. Use few broad shapes, no detailed feathers or noisy texture. Let the whole owl fill about 78 percent of the square height, optically centered with a clean silhouette and generous clear margins. Transparent background with actual alpha. No tile, no backdrop, no branch, no other props, no letters or text, no UI, no checkerboard, no external shadow or glow. The subject's dark features must be opaque.
```
