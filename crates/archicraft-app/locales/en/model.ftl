### ArchiCraft English message catalogue: the headless model (NFR-L10N-001).
###
### archicraft-model has no catalogue of its own: command labels, validation
### and command errors are message keys starting with model-, which the
### application looks up here, often built at runtime.

## Command labels (Command::label), shown in the Edit menu as "Undo …" and
## in the status line. $kind is an element kind noun (model-kind-*-noun).

model-label-create-kind = Create { $kind }
model-label-create-elements = Create { $count ->
        [one] { $count_text } element
       *[other] { $count_text } elements
    }
model-label-delete-elements = Delete { $count ->
        [one] { $count_text } element
       *[other] { $count_text } elements
    }
model-label-edit-elements = Edit { $count ->
        [one] { $count_text } element
       *[other] { $count_text } elements
    }
model-label-move-elements = Move { $count ->
        [one] { $count_text } element
       *[other] { $count_text } elements
    }
model-label-insert-storey = Insert storey
model-label-delete-storey = Delete storey
model-label-rename-storey = Rename storey
model-label-storey-elevation = Change storey elevation
model-label-storey-height = Change storey height
model-label-cut-plane = Change cut plane
model-label-new-layer = New layer
model-label-change-layer = Change layer
model-label-delete-layer = Delete layer
model-label-new-material = New material
model-label-change-material = Change material
model-label-delete-material = Delete material
model-label-tool-settings = Change tool settings
model-label-project-settings = Change project settings
model-label-project-info = Change project information
model-label-spatial-names = Rename site and building
model-label-location = Change project location

## Element kinds: the label (a name on its own, such as a heading) and the
## noun inside a sentence, with $count selecting the plural.

model-kind-wall = Wall
model-kind-wall-noun = { $count ->
        [one] wall
       *[other] walls
    }
model-kind-slab = Slab
model-kind-slab-noun = { $count ->
        [one] slab
       *[other] slabs
    }
# The test kind of the model's test builds; never shown in a release.
model-kind-test = Test element
model-kind-test-noun = { $count ->
        [one] test element
       *[other] test elements
    }

## Command errors (CommandError::message), shown in the status line after
## the command label. Lengths ($min, $max) are formatted in the project
## length unit with its symbol, angles in the project angle unit; $name,
## $lower, $upper and $layer are element, storey or layer names; $reason is
## another error message.

model-error-no-change = Nothing to change.
model-error-invalid-distance = Enter a valid distance.
model-error-element-not-found = The element no longer exists.
model-error-duplicate-element-id = An element with this ID already exists.
model-error-element-invalid = { $name }: { $reason }
model-error-kind-change = An element cannot change its kind.
model-error-element-not-movable = { $name } cannot be moved.
model-error-transform-unsupported = { $name } cannot be transformed like this.
model-error-duplicate-storey-id = A storey with this ID already exists.
model-error-move-target-same = Choose another storey to move the elements to.
model-error-target-storey-not-found = The target storey does not exist.
model-error-duplicate-layer-id = A layer with this ID already exists.
model-error-layer-not-found = The layer does not exist.
model-error-layer-name = A layer needs a name.
model-error-reassign-layer = Choose another layer for the elements.
model-error-duplicate-material-id = A material with this ID already exists.
model-error-material-not-found = The material does not exist.
model-error-replacement-material = Choose a replacement material.
model-error-default-layer-missing = The default layer does not exist.
model-error-default-material-missing = The default material does not exist.
model-error-default-ifc-class = The default IFC class is not allowed for this tool.
model-error-cut-plane-range = The cut-plane height must be within ±{ $max }.
model-error-tessellation-range = The tessellation tolerances are out of range.
model-error-display-precision = Display precision is limited to { $max_text } decimals.
model-error-project-name = The project needs a name.
model-error-spatial-names = Site and building need names.
model-error-location-invalid = The location has invalid values.

## Element validation (ELM-017, EDT-018, EDT-019). $kind is an element kind
## noun, $class an IFC class name such as IfcWall.

model-error-coordinates-range = Coordinates must be finite and within { $max } of the project origin.
model-error-polygon-points = A polygon needs at least three distinct points.
model-error-polygon-area = The polygon has no area.
model-error-polygon-sliver = The polygon has a sliver corner sharper than { $min }.
model-error-polygon-self-intersection = The polygon outline intersects itself.
model-error-hole = Hole: { $reason }
model-error-hole-outside = A hole must lie inside the slab outline without touching it.
model-error-holes-overlap = Holes must not overlap each other.
model-error-slab-points = A slab may have at most { $max_text } points in its outline and holes together.
model-error-thickness-range = Thickness must be between { $min } and { $max }.
model-error-base-offset-range = The base offset must be a finite value within ±{ $max }.
model-error-top-offset-range = The top offset must be a finite value within ±{ $max }.
model-error-slab-offset-range = The slab offset must be a finite value within ±{ $max }.
model-error-wall-length = A wall must be at least { $min } long.
model-error-reference-line-range = The reference-line offset must be within ±{ $max }.
model-error-wall-height-range = The wall height must be between { $min } and { $max }.
model-error-linked-storey-range = The linked storey must be at most { $max_text } storeys away.
model-error-wall-top-storey-missing = The storey the wall top is linked to does not exist.
model-error-element-name = An element needs a name.
model-error-home-storey-missing = The element's home storey does not exist.
model-error-element-layer-missing = The element's layer does not exist.
model-error-element-material-missing = The element's material does not exist.
model-error-ifc-class-not-allowed = { $class } is not an allowed IFC class for a { $kind }.
model-error-vertical-extent = The element's vertical extent cannot be determined.
model-error-top-below-base = The top of the element must be above its base.

## Storeys (STR-001…006).

model-error-no-storeys = A building needs at least one storey.
model-error-ground-not-in-list = The ground storey is not in the storey list.
model-error-storeys-too-close = Storeys “{ $lower }” and “{ $upper }” are less than { $min } apart.
model-error-storey-invalid-values = Storey “{ $name }” has an invalid elevation or height.
model-error-storey-name = A storey needs a name.
model-error-storey-cut-plane = The cut-plane height of storey “{ $name }” must be within ±{ $max }.
model-error-storey-height-min = Storey heights must be at least { $min }.
model-error-storey-unknown = The storey does not exist.
model-error-last-storey = The last storey cannot be deleted.
model-error-no-ground-left = No storey is left to become the ground storey.
model-error-invalid-elevation = Enter a valid elevation.
model-error-storey-below = The storey would move below the storey underneath it.
model-error-storey-above = The storey would move above the storey over it.
model-error-storey-collision = The storey above would collide with the one over it.

## Materials (MAT-001).

model-error-material-name = A material needs a name.
model-error-opacity-range = Opacity must be between 0 and 1.
model-error-priority-range = Intersection priority must be between { $min_text } and { $max_text }.
model-error-colour-range = Colour components must be between 0 and 1.

## Plan grid (INP-013), locks (ELM-013, LAY-001) and the integrity problems
## of a project (NFR-REL-004).

model-error-snap-grid-range = The snap grid steps must be greater than 0 and at most { $max }.
model-error-construction-grid-range = The construction grid spacings must be greater than 0 and at most { $max }.
model-error-auxiliary-spacing = The auxiliary grid spacing must not be larger than the main spacing.
model-error-grid-rotation = The grid rotation must be a finite number.
model-error-grid-origin = The grid origin must be within { $max } of the project origin.
model-error-element-locked = { $name } is locked.
model-error-layer-locked = { $name } is on the locked layer “{ $layer }”.
model-problem-home-storey-missing = { $name }: the home storey is missing.
model-problem-layer-missing = { $name }: the layer is missing.
model-problem-material-missing = { $name }: the material is missing.
