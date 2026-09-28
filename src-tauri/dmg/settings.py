# The installer's window, for dmgbuild. scripts/release.sh runs it with -D app=<Codealong.app>.
# dmgbuild writes the window's layout into the disk image's .DS_Store itself rather than asking
# Finder to arrange a window, so the image builds the same on any Mac, with no window opening and
# no permission to control Finder. background.png is drawn by scripts/dmg-background.mjs to fit
# the positions below; move one and redraw it.
import os.path

app = defines["app"]
name = os.path.basename(app)
here = defines["dir"]

files = [app]
symlinks = {"Applications": "/Applications"}
# No hide_extensions: hiding ".app" is a Finder-info attribute on the app's folder, which strict
# signature checks refuse ("detritus not allowed"), and Finder hides it for apps anyway.
icon = os.path.join(app, "Contents/Resources/icon.icns")
background = os.path.join(here, "background.png")

# LZFSE: smaller than zlib and quick to open; every macOS Codealong runs on reads it.
format = "ULFO"
filesystem = "HFS+"

# The size includes the title bar: the background shows about 368 points below it.
window_rect = ((200, 160), (660, 400))
default_view = "icon-view"
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
include_list_view_settings = False
arrange_by = None
icon_size = 128
text_size = 13
label_pos = "bottom"
icon_locations = {name: (180, 156), "Applications": (480, 156)}
