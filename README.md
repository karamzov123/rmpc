# Rusty Music Player Client

Rmpc is a beautiful, modern and configurable terminal based Music Player Daemon client. It is
heavily inspired by [ncmpcpp](https://github.com/ncmpcpp/ncmpcpp) and
[ranger](https://github.com/ranger/ranger)/[lf](https://github.com/gokcehan/lf) file managers.

![preview](assets/preview.png)

## Fork Features & Enhancements

This fork introduces enhancements to the user interface, library navigation, and companion workflow scripts:

- **Bouncing text scroll with custom pauses**: Long titles and tags bounce smoothly between start and end boundaries, resetting scroll progress whenever moving between tracks and pausing gracefully at edges.
- **Wall-clock paused playback scrolling**: Scrolling labels in property panes and status lines remain animated even when playback is paused or stopped.
- **Adaptive 2-column & 3-column layouts**:
  - The search pane automatically detects 2-column mini mode (`browser_column_widths[2] == 0`) and allocates inputs and results cleanly across the available columns.
  - Search and album browser panes start flush against the far-left border.
- **Dynamic album folder icon collapse**: Navigating inside an album collapses the outer artist column down into a compact album folder icon, granting equal 50/50 space to tracks and metadata.
- **Clean metadata text wrapping**: Long metadata fields (paths, titles, tags, and stickers) in preview panes wrap cleanly onto multiple indented lines rather than clipping.
- **Collaborative artist & featuring grouping**:
  - Featured artists (`feat.`, `ft.`, `featuring`) automatically collapse under the primary artist in the album view.
  - Multi-artist collaborative albums (e.g. `Duke Ellington & John Coltrane`) appear under both artists if each has recordings in the library.
- **Hover album art preview**: Dynamic cover art previews when hovering artists and search results.
- **Companion scripts**: Helper scripts under `scripts/` for dynamic theme swapping on resize, lyrics fetching, live playback lyric stamping (`lrc-stamper.py`), and play count tracking.

## Get started

Description, configuration and installation methods can be found on [the rmpc website](https://rmpc.mierak.dev)

## Main Features

- Album cover art display if your terminal supports either of Kitty, Sixel, Iterm2 protocols, or via ueberzuggpp
- Cava integration for music visualisation
- Support for [synchronized lyrics](https://en.wikipedia.org/wiki/LRC_(file_format))
- Ability to play music from YouTube
- Configurable (T)UI
  - Configure what information(if any!) is displayed in the header
  - Configure what columns are displayed on the queue screen
  - Create any color theme you want
  - Every keybind can be changed, vim-like by default
- Ranger/LF-like three-column browser through your music library
- Basic playlist management
- Support scripting through basic CLI mode and script hooks

And more to come

## Discussion

* Open a discussion in the [repository](https://github.com/mierak/rmpc/discussions)
* Join the Discord server: https://discord.gg/9MAddPFqSF

## Status

Rmpc is still maturing. It is stable for daily use, although breaking changes, while rare, are sometimes unavoidable.

## Star History

<a href="https://star-history.dera.page/#mierak/rmpc&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://star-history.dera.page/svg?repos=mierak/rmpc&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://star-history.dera.page/svg?repos=mierak/rmpc&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://star-history.dera.page/svg?repos=mierak/rmpc&type=date&legend=top-left" />
 </picture>
</a>

## Packaging status
[![Packaging status](https://repology.org/badge/vertical-allrepos/rmpc.svg)](https://repology.org/project/rmpc/versions)
