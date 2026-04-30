# VHS tape for the lazyrecall demo GIF.
#
# Install VHS once: `brew install vhs` (or see github.com/charmbracelet/vhs).
# Render with:      `vhs docs/demo.tape`
# Output:           `docs/demo.gif`
#
# This file is the canonical source for the README's demo. Keep it in sync
# with the README's claims: any feature shown in the GIF should be a real
# feature today.

Output docs/demo.gif

Set FontSize 16
Set Width 1200
Set Height 720
Set Theme "Catppuccin Mocha"
Set Padding 20
Set TypingSpeed 50ms

# Boot the TUI.
Type "lazyrecall"
Enter
Sleep 2s

# Show the projects pane: arrow down a few times so the viewer sees the
# recency-sorted list and the inline summaries appearing.
Down
Sleep 800ms
Down
Sleep 800ms
Down
Sleep 800ms
Up
Sleep 600ms

# Cycle into the Sessions pane.
Tab
Sleep 1s

# Walk through a few sessions to show the preview pane updating.
Down
Sleep 1s
Down
Sleep 1s
Down
Sleep 1s
Up
Up
Sleep 800ms

# Cycle into the Preview pane to show the focused border + scrollable preview.
Tab
Sleep 1500ms

# Back to Sessions, ready to resume.
Tab
Sleep 800ms

# End on the sessions pane with one selected — the user understands the
# next keystroke would be Enter to drop into `claude --resume`.
Sleep 1s
