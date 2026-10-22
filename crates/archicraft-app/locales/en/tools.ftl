### ArchiCraft English message catalogue: drawing tools (NFR-L10N-001,
### NFR-USE-003).
###
### Tool names and descriptions start with tool-, the input prompts of the
### status line with prompt-, the field labels of the tracker with tracker-.

## Tracker field labels (INP-002, INP-004): polar distance and angle,
## relative ΔX ΔY ΔZ, absolute X Y Z and, while stretching, the total
## length and the length change.

tracker-distance = d
tracker-angle = a
tracker-dx = ΔX
tracker-dy = ΔY
tracker-dz = ΔZ
tracker-x = X
tracker-y = Y
tracker-z = Z
tracker-total = L
tracker-change = ΔL

## Placing the user origin (INP-014): the prompts of its two steps and, as
## prompt-user-origin-key-*, the key hints shown after them.

prompt-user-origin-point = User origin: click the new origin
prompt-user-origin-direction = User origin: click a point on the X axis or type its angle
prompt-user-origin-key-coordinates = types the coordinates
prompt-user-origin-key-keep-rotation = keeps the axes unrotated
prompt-user-origin-key-constrain = constrains angles
prompt-user-origin-key-cancel = cancels the step

## The division control of the drawing tools (INP-010): the one active
## division mode, its value and the scope of the division points shown on a
## hovered edge.

tool-division = Division
tool-division-hint = The snap points shown on a hovered edge: one division mode, its value and the stretch it divides
tool-division-half = Half
tool-division-equal = Equal parts
tool-division-percent = Percentage
tool-division-percent-suffix = % from the nearer end
tool-division-repeated = Repeated distance
tool-division-best-equal = Equal parts near a distance
tool-division-distance-positive = The division distance must be greater than zero.
tool-division-scope-nodes = Between nodes
tool-division-scope-intersections = Between intersections
tool-division-scope-element = Whole element
