# Codealong mark

The selected mark is a rounded C surrounding a circular commit node with a short,
rounded stem. Keep it flat, with a neutral C and a colored node for live status.
The [status sheet](preview.html) shows all five states in light, dark, and Dock form.

| Node | Meaning |
| --- | --- |
| Red | A session has an error |
| Amber | A session needs you, or a PR awaits your review |
| Blue | An agent finished and you have not looked yet |
| Green | Agents are working |
| Light gray | Idle |

Priority is error → needs you → your turn → working → idle. The engine computes
one status across all visible projects, ignoring archived sessions and duplicate
visitors. Project, search and age filters do not change it. Every window, the
floating bar, browser icon and running Dock icon use that same status. The installed
Finder icon uses the light gray idle node. UI and browser marks use the matching light/dark tokens;
the Dock keeps its dark tile and uses the dark palette.

`mark.svg` is the production vector master. `chosen-reference.png` preserves the
approved concept, #115 “Node · soft,” selected September 25, 2026. The vector
regularizes its curves and centers the silhouette for use at icon sizes.

Run `npm run icons` after editing the master. It generates:

- `src/ui/brand.ts`, used by the project switcher and floating bar through `Mark`.
- `public/favicon.svg` and `public/logos/*`, which follow the browser's appearance.
- `design/app-icon.svg`, the flat Dock/Finder icon using the dark surface and ink tokens.
- `src-tauri/app-icon.png` and `src-tauri/icons/*`, the native bundle assets.
- `src-tauri/icons/status/*`, the running Dock's colored states.
- `preview.html`, the compact status reference sheet.

The exploratory galleries were removed from the current tree. All rounds, prompts,
and comparisons remain in Git: `5c4571e` contains the final playground, and
`62c363d` introduced the chosen concept at
`design/logo-concepts/2026-09-24-soft-c/115-node-soft.png`.
