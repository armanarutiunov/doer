# Agents

## TermUI rendering quirk

Adjacent rows with identical RGB styles get merged into a single render span, breaking scroll rendering. Fix: alternate the blue channel by 1 using `rem(idx, 2)` to force unique styles per row (e.g. `{100, 100, 100 + rem(idx, 2)}`).
