# Accessibility

raiox should work for people who use a keyboard, a screen reader, or reduced
motion. What it does today:

- Controls are real buttons, links and fields, reachable with the keyboard,
  with a visible focus ring.
- Pickers and switches carry their role and state (radio groups, switches,
  sliders), and icon-only buttons have a label.
- Animations stop when the system asks for reduced motion.
- Text contrast is at least 7:1: grey support text 7.0:1, white text 17:1.
- Charts drawn as pixels are hidden from screen readers, and each one can switch
  to a plain table.

Known limits:

- The panel is dark only; there is no light theme yet.
- The usage clock and the context ruler are visual; their values are in the
  text beside them, not in a table. The clock's hours open with a mouse only.

Found a barrier? Open an issue with the screen, what you used (keyboard, screen
reader and which, zoom level) and what got in the way.
