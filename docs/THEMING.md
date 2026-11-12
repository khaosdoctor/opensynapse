# Theming

What you pick on the settings tab is saved in `~/.config/opensynapse/settings.json`, along with the ***Show raw bytes in the log*** checkbox from its debug section. The log panel at the bottom of the sidebar uses a black background and the theme's accent color for timestamps.

## Mode

***SYSTEM*** follows your desktop's light or dark setting. Every theme has both versions.

Tiling window managers like Sway or Hyprland often don't report a preference, so ***SYSTEM*** stays dark there. Pick ***LIGHT***, or put `color-scheme: light;` in `theme.css` so ***SYSTEM*** uses that instead.

## Themes

| Theme | Look |
|---|---|
| OpenSynapse | the default, neon on near-black |
| Synapse | Razer's own app, black and green |
| Adwaita | GNOME |
| Breeze | KDE |
| Catppuccin | Latte and Mocha |
| Kanagawa | Lotus and Wave |
| Dracula | Alucard and Dracula |

## Your own theme

Themes are CSS files, the built-in ones are in [`themes/`](../themes). Yours go in `~/.config/opensynapse/themes/<name>.css`. They show up after ***RELOAD***, and a file named like a built-in theme replaces it.

Only custom properties are read. Anything outside a `prefers-color-scheme` block counts for both modes.

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

[`themes/opensynapse.css`](../themes/opensynapse.css) lists every property. `--hypershift` is the color of the Hypershift layer, and a theme that leaves it out uses its `--warn`.

## Small changes

`~/.config/opensynapse/theme.css` takes the same properties and applies on top of any theme.

## Font

***THEME DEFAULT*** uses the theme's `--font`. Any other font in the list replaces it for every theme.
