# Theming

What you pick on the settings tab is saved in `~/.config/opensynapse/settings.json`, along with the log options from its debug section, and the log font size there overwrites the theme's `--log-font-size`.

## Mode

***SYSTEM*** follows your desktop's light or dark setting, and every theme has both versions. Tiling window managers like Sway or Hyprland often don't report a preference, so ***SYSTEM*** stays dark there, and if you want light you can pick ***LIGHT*** or put `color-scheme: light;` in `theme.css` so ***SYSTEM*** uses that instead.

## Themes

| Theme | Look |
|---|---|
| OpenSynapse | the default, cyberpunk |
| Synapse | Razer's own app, black and green |
| Adwaita | GNOME |
| Breeze | KDE |
| Catppuccin | Latte and Mocha |
| Kanagawa | Lotus and Wave |
| Dracula | Alucard and Dracula |

## Your own theme

Themes are CSS files and the built-in ones are in [`themes/`](../themes), so if you want a new one you can create it in the GUI with ***NEW THEME*** on the settings tab, which starts it as a copy of the default one, or create a file in `~/.config/opensynapse/themes/<name>.css` yourself, and it shows up after ***RELOAD***, replacing any built-in theme with the same name.

The app only reads custom properties, and anything you put outside a `prefers-color-scheme` block counts for both modes.

```css
:root {
  --radius: 6;
  --font: "Noto Sans";
  --accent: #00f0ff;
}
@media (prefers-color-scheme: light) {
  :root { --accent: #007a8a; }
}
```

[`themes/opensynapse.css`](../themes/opensynapse.css) lists every property, like `--hypershift`, the color of the Hypershift layer, and if your theme leaves it out it uses the theme's `--warn` instead. The LOG panel has its own `--log-bg`, `--log-text`, `--log-raw`, `--log-time` and `--log-font-size`, and if you leave any of them out it falls back to the theme's matching color or to 10px.

## Small changes

If you only want to change a couple of things, `~/.config/opensynapse/theme.css` takes the same properties and applies them on top of whatever theme you picked.

## Font

***THEME DEFAULT*** uses the theme's `--font`, and if you pick any other font in the list, it replaces that for every theme.

## Gallery

This is the lighting tab in the default theme.

| Dark | Light |
|---|---|
| ![The lighting tab, dark](screenshots/lighting.png) | ![The lighting tab, light](screenshots/lighting-light.png) |

And these are all the built-in themes on the buttons tab.

| Theme | Dark | Light |
|---|---|---|
| opensynapse | ![opensynapse, dark](screenshots/themes/opensynapse-dark.png) | ![opensynapse, light](screenshots/themes/opensynapse-light.png) |
| synapse | ![synapse, dark](screenshots/themes/synapse-dark.png) | ![synapse, light](screenshots/themes/synapse-light.png) |
| adwaita | ![adwaita, dark](screenshots/themes/adwaita-dark.png) | ![adwaita, light](screenshots/themes/adwaita-light.png) |
| breeze | ![breeze, dark](screenshots/themes/breeze-dark.png) | ![breeze, light](screenshots/themes/breeze-light.png) |
| catppuccin | ![catppuccin, dark](screenshots/themes/catppuccin-dark.png) | ![catppuccin, light](screenshots/themes/catppuccin-light.png) |
| kanagawa | ![kanagawa, dark](screenshots/themes/kanagawa-dark.png) | ![kanagawa, light](screenshots/themes/kanagawa-light.png) |
| dracula | ![dracula, dark](screenshots/themes/dracula-dark.png) | ![dracula, light](screenshots/themes/dracula-light.png) |
