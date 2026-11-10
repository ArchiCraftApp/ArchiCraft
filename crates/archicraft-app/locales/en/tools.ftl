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

## The measure tool (MEA-001): its modes, the prompts of its steps with
## their key hints as prompt-measure-key-*, and the labels of its readout in
## the status line, beside the tracker's ΔX, ΔY and direction a.

tool-measure-mode-distance = Distance
tool-measure-mode-angle = Angle
tool-measure-total = Total
tool-measure-segment = Segment
tool-measure-angle = Angle
prompt-measure-distance-first = Measure distance: click the first point
prompt-measure-distance-next = Measure distance: click the next point or type a distance
prompt-measure-angle-vertex = Measure angle: click the vertex
prompt-measure-angle-first-leg = Measure angle: click a point on the first leg
prompt-measure-angle-second-leg = Measure angle: click a point on the second leg
prompt-measure-key-finish = or double-click finishes the measurement
prompt-measure-key-remove-last = removes the last point
prompt-measure-key-clear = clears the measurement

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

## Contour detection of the wall and slab tools (INP-018): the prompts of its
## pick step as prompt-contour-*, its key hints and the hint of the held key
## on the tools' first point as prompt-contour-key-*.

prompt-contour-slab = Slab from a contour: click inside a region or on a closed chain of edges
prompt-contour-wall = Walls from a contour: click on an edge or a chain of edges, or inside a region
prompt-contour-key-hold = held traces a contour
prompt-contour-key-edge = picks the clicked edge only
prompt-contour-key-cancel = turns contour detection off

## The space tool (SPC-001): the default long name of a new space, $n its
## number among the spaces (ELM-019); the prompts of detecting a room and of
## drawing its boundary.

tool-space-default-name = Room { $n }
prompt-space-detect = Space: click inside a room bounded by walls
prompt-space-region-open = Region is open: click inside a room closed by walls
prompt-space-in-wall = The point lies in a wall: click inside a room
prompt-space-polygon-first = Space: click the first corner of the boundary
prompt-space-polygon-next = Space: click the next corner or type a distance
prompt-space-rectangle-first = Space rectangle: click the first corner
prompt-space-rectangle-next = Space rectangle: click the opposite corner or type a distance

## The text tool (ANN-007): placing a text's anchor, then typing its content
## in the inline editor, and the keys of the editor.

prompt-text-anchor = Text: click where the text goes or type its coordinates
prompt-text-type = Text: type the text, a click outside places it
prompt-text-key-newline = starts a new line
prompt-text-key-commit = places the text
prompt-text-key-cancel = cancels the text
