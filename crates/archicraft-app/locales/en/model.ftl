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
model-label-inject-settings = Inject settings into { $count ->
        [one] { $count_text } element
       *[other] { $count_text } elements
    }
model-label-pick-up-settings = Pick up settings
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
model-label-new-profile = New profile
model-label-change-profile = Change profile
model-label-delete-profile = Delete profile
model-label-tool-settings = Change tool settings
model-label-project-settings = Change project settings
model-label-project-info = Change project information
model-label-spatial-names = Rename site and building
model-label-location = Change project location
model-label-group = Group
model-label-ungroup = Ungroup
model-label-rename-group = Rename group
model-label-insert-relation = Add relation
model-label-remove-relation = Remove relation

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
model-kind-space = Space
model-kind-space-noun = { $count ->
        [one] space
       *[other] spaces
    }
model-kind-text = Text
model-kind-text-noun = { $count ->
        [one] text
       *[other] texts
    }
# The test kind of the model's test builds; never shown in a release.
model-kind-test = Test element
model-kind-test-noun = { $count ->
        [one] test element
       *[other] test elements
    }
# The body-less test kind of the model's test builds; never shown in a
# release.
model-kind-test-annotation = Test annotation
model-kind-test-annotation-noun = { $count ->
        [one] test annotation
       *[other] test annotations
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
model-error-transfer-unsupported = None of these settings apply here.
model-error-duplicate-storey-id = A storey with this ID already exists.
model-error-move-target-same = Choose another storey to move the elements to.
model-error-target-storey-not-found = The target storey does not exist.
model-error-linked-element-too-low = { $count ->
        [0] { $name } would be less than { $min } high.
        [one] { $name } and { $count_text } other element would be less than { $min } high.
       *[other] { $name } and { $count_text } other elements would be less than { $min } high.
    }
model-error-linked-element-too-high = { $count ->
        [0] { $name } would be more than { $max } high.
        [one] { $name } and { $count_text } other element would be more than { $max } high.
       *[other] { $name } and { $count_text } other elements would be more than { $max } high.
    }
model-error-duplicate-layer-id = A layer with this ID already exists.
model-error-layer-not-found = The layer does not exist.
model-error-layer-name = A layer needs a name.
model-error-reassign-layer = Choose another layer for the elements.
model-error-duplicate-material-id = A material with this ID already exists.
model-error-material-not-found = The material does not exist.
model-error-replacement-material = Choose a replacement material.
model-error-duplicate-profile-id = A profile with this ID already exists.
model-error-profile-not-found = The profile does not exist.
model-error-replacement-profile = Choose another existing profile as the replacement.
model-error-profile-in-use = { $count ->
        [0] Profile “{ $name }” is used by tool defaults; choose a replacement.
        [one] Profile “{ $name }” is used by { $count_text } element; choose a replacement.
       *[other] Profile “{ $name }” is used by { $count_text } elements; choose a replacement.
    }
model-error-default-missing = The { $kind } tool has no defaults.
model-error-default-invalid = The { $kind } tool defaults are invalid: { $reason }
model-error-cut-plane-range = The cut-plane height must be within ±{ $max }.
model-error-tessellation-range = The tessellation tolerances are out of range.
model-error-display-precision = Display precision is limited to { $max_text } decimals.
model-error-project-name = The project needs a name.
model-error-spatial-names = Site and building need names.
model-error-location-invalid = The location has invalid values.
model-error-duplicate-group-id = A group with this ID already exists.
model-error-group-not-found = The group does not exist.
model-error-group-name = A group needs a name.
model-error-group-change = An element joins or leaves a group only through Group and Ungroup.
# Relations between elements (SRS 3.3). $relation is the name of the kind of
# relation (model-relation-*), $name the element that depends on the other.
model-error-relation-derived = Storey links follow from the elements' storeys and cannot be added or removed.
model-error-relation-not-found = The relation does not exist.
model-error-relation-end-missing = The { $relation } relation refers to an element that does not exist.
model-error-relation-self = { $name } cannot have a { $relation } relation with itself.
model-error-relation-duplicate = { $name } already has this { $relation } relation.
model-error-host-twice = { $name } already has a host.
model-error-host-cycle = { $name } cannot be hosted by an element it hosts.

## Element validation (ELM-017, EDT-018, EDT-019). $kind is an element kind
## noun, $class an IFC class name such as IfcWall.

model-error-coordinates-range = Coordinates must be finite and within { $max } of the project origin.
model-error-polygon-points = A polygon needs at least three distinct points.
model-error-polygon-area = The polygon has no area.
model-error-polygon-sliver = The polygon has a sliver corner sharper than { $min }.
model-error-polygon-self-intersection = The polygon outline intersects itself.
model-error-hole = Hole: { $reason }
model-error-hole-outside = A hole must lie inside the outline without touching it.
model-error-holes-overlap = Holes must not overlap each other.
model-error-slab-points = A slab may have at most { $max_text } points in its outline and holes together.
model-error-space-points = A space may have at most { $max_text } points in its boundary and holes together.
model-error-thickness-range = Thickness must be between { $min } and { $max }.
model-error-base-offset-range = The base offset must be a finite value within ±{ $max }.
model-error-top-offset-range = The top offset must be a finite value within ±{ $max }.
model-error-slab-offset-range = The slab offset must be a finite value within ±{ $max }.
model-error-wall-length = A wall must be at least { $min } long.
model-error-reference-line-range = The reference-line offset must be within ±{ $max }.
model-error-wall-height-range = The wall height must be between { $min } and { $max }.
model-error-space-height-range = The space height must be between { $min } and { $max }.
model-error-linked-storey-range = The linked storey must be at most { $max_text } storeys away.
model-error-wall-top-storey-missing = The storey the wall top is linked to does not exist.
model-error-element-name = An element needs a name.
model-error-home-storey-missing = The element's home storey does not exist.
model-error-element-layer-missing = The element's layer does not exist.
model-error-element-material-missing = The element's material does not exist.
model-error-element-profile-missing = The element's profile does not exist.
model-error-profile-usage = Profile “{ $name }” may not be used by a { $kind }.
model-error-ifc-class-not-allowed = { $class } is not an allowed IFC class for a { $kind }.
model-error-predefined-type-not-allowed = { $type } is not a predefined type of { $class }.
model-error-vertical-extent = The element's vertical extent cannot be determined.
model-error-top-below-base = The top of the element must be above its base.
# A 2D element (text, dimension) belongs to the plan view of its home
# storey; $view and $home are storey names.
model-error-view-storey-missing = The storey of the element's view does not exist.
model-error-view-not-home-storey = The element is in the plan view of { $view }, but 2D elements belong to the plan view of their own storey, { $home }.

## Texts and text styles (ANN-007). $name is a text style name.

model-error-text-content-empty = A text needs at least one visible character.
model-error-text-angle = The text angle must be a finite number.
model-error-text-size = The text size must be a positive number of millimetres.
model-error-text-style-missing = The text style of the element does not exist.
model-error-text-style-name = A text style needs a name.
model-error-text-style-size = The size of the text style { $name } must be a positive number of millimetres.
model-error-text-style-line-spacing = The line spacing of the text style { $name } must be a positive number.
model-error-text-style-duplicate-name = There is more than one text style named { $name }.
model-error-duplicate-text-style-id = The text style { $name } has the identity of another text style.

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
model-problem-text-style-missing = { $name }: the text style is missing.
model-problem-relation-end-missing = { $name }: its { $relation } relation refers to a missing element.
model-problem-relation-ends-missing = A { $relation } relation refers to two missing elements.
model-problem-profile-material-missing = Profile “{ $name }”: a material is missing.

## Profiles (PRF-004, PRF-005), from validate_profile. $dimension is a
## dimension name below, $number, $first and $second count regions from 1.

model-error-profile-name = A profile needs a name.
model-error-profile-origin = The profile origin must be within { $max } of the profile's zero point.
model-error-profile-material-missing = The profile uses a material that does not exist.
model-error-profile-dimension-range = { $dimension } must be between { $min } and { $max }.
model-error-profile-dimension-limit = { $dimension } must be less than { $max }.
model-error-profile-radius-limit = { $dimension } must be at most { $max }.
model-error-profile-no-regions = A custom profile needs at least one region.
model-error-profile-points = A profile may have at most { $max_text } points in all its regions.
model-error-profile-regions = A custom profile may have at most { $max_text } regions.
model-error-profile-size = A profile may be at most { $max } wide and { $max } tall.
model-error-profile-position = The regions of a custom profile must lie within { $max } of the profile's zero point in each direction.
model-error-profile-crossings = The outlines of a custom profile's regions may cross each other within the model tolerance in at most { $max_text } places.
model-error-profile-region = Region { $number_text }: { $reason }
model-error-profile-regions-overlap = Regions { $first_text } and { $second_text } overlap; regions may only touch.

## Parametric profile shapes and their dimensions (PRF-005), from label_key

model-profile-shape-rectangle = Rectangle
model-profile-shape-circle = Circle
model-profile-shape-rectangular-hollow = Rectangular hollow section
model-profile-shape-circular-hollow = Circular hollow section
model-profile-shape-i = I/H section
model-profile-shape-t = T section
model-profile-shape-l = Angle (L)
model-profile-shape-u = Channel (U/C)
model-profile-shape-z = Z section
model-profile-dimension-width = Width
model-profile-dimension-height = Height
model-profile-dimension-diameter = Diameter
model-profile-dimension-thickness = Thickness
model-profile-dimension-outer-radius = Outer radius
model-profile-dimension-web = Web thickness
model-profile-dimension-flange = Flange thickness
model-profile-dimension-root-radius = Root radius
model-profile-dimension-toe-radius = Toe radius

## Relations (SRS 3.3), from Relation::label_key

model-relation-storey-link = storey link
model-relation-host = host
model-relation-junction = junction
model-relation-trim = trim
model-relation-solid-operation = solid operation
model-relation-annotation-binding = annotation binding

## Typed lengths, angles and slopes (INP-005, NFR-L10N-003). $text is what the user typed.

model-error-parse-empty = Enter a number.
model-error-parse-length = “{ $text }” is not a valid length.
model-error-parse-angle = “{ $text }” is not a valid angle.
model-error-parse-slope = “{ $text }” is not a valid slope.

## Drawing scales of 2D views (VIEW-013). $text is what the user typed, $max
## the largest denominator.

model-error-parse-scale = “{ $text }” is not a drawing scale; enter 1:50 or 50.
model-error-scale-range = A drawing scale runs from 1:1 to 1:{ $max }.

## Unit names in the project settings (STR-009).

model-unit-length-m = Metre
model-unit-length-cm = Centimetre
model-unit-length-mm = Millimetre
model-unit-length-ft-in = Feet and inches
model-unit-length-ft = Decimal feet
model-unit-length-in = Decimal inches
model-unit-angle-deg = Decimal degrees
model-unit-angle-dms = Degrees, minutes, seconds
model-unit-angle-grad = Gradians
model-unit-angle-rad = Radians
model-unit-slope-deg = Degrees
model-unit-slope-percent = Percent
model-unit-slope-ratio = Ratio
model-unit-area-m2 = Square metres
model-unit-area-ft2 = Square feet
model-unit-volume-m3 = Cubic metres
model-unit-volume-ft3 = Cubic feet

## IFC classes, shown by their IFC 4.3 names, and the locations of a wall's
## reference line (WAL-003).

model-ifc-class-proxy = IfcBuildingElementProxy
model-ifc-class-wall = IfcWall
model-ifc-class-slab = IfcSlab
model-ifc-class-roof = IfcRoof
model-ifc-class-column = IfcColumn
model-ifc-class-beam = IfcBeam
model-ifc-class-member = IfcMember
model-ifc-class-plate = IfcPlate
model-ifc-class-covering = IfcCovering
model-ifc-class-footing = IfcFooting
model-ifc-class-pile = IfcPile
model-ifc-class-railing = IfcRailing
model-ifc-class-stair = IfcStair
model-ifc-class-ramp = IfcRamp
model-ifc-class-chimney = IfcChimney
model-ifc-class-shading-device = IfcShadingDevice
model-ifc-class-curtain-wall = IfcCurtainWall
model-ifc-class-furniture = IfcFurniture
model-ifc-class-space = IfcSpace
model-ifc-class-geographic-element = IfcGeographicElement
model-ifc-class-stair-flight = IfcStairFlight
model-ifc-class-opening-element = IfcOpeningElement
model-ifc-class-door = IfcDoor
model-ifc-class-window = IfcWindow
model-ifc-class-grid = IfcGrid
model-ifc-class-annotation = IfcAnnotation
model-reference-line-outside = Exterior face
model-reference-line-center = Centre
model-reference-line-inside = Interior face
model-reference-line-custom = Custom offset

## Predefined types of IFC classes (IFX-024).

model-predefined-type-floor = Floor
model-predefined-type-roof = Roof
model-predefined-type-landing = Landing
model-predefined-type-base-slab = Base slab
model-predefined-type-pad-footing = Pad footing
model-predefined-type-strip-footing = Strip footing

## Structural functions of elements (ELM-020).

model-structural-function-undefined = Undefined
model-structural-function-load-bearing = Load-bearing
model-structural-function-non-load-bearing = Non-load-bearing

## Field labels and option labels (fields.rs, KindDescriptor::fields).
model-field-material = Material
model-field-layer = Layer
model-field-ifc-class = IFC class
model-field-predefined-type = Predefined type
model-field-position = Position
model-field-structural-function = Structural function
model-field-thickness = Thickness
model-field-reference-line = Reference line
model-field-reference-offset = Offset from exterior face
model-field-base-offset = Base offset
model-field-top-mode = Top
model-field-height = Height
model-field-top-storey = Top storey
model-field-linked-storey = Linked storey
model-field-top-offset = Top offset
model-field-slab-reference = Reference plane
model-field-elevation-offset = Elevation offset
model-field-room-name = Room name
model-field-default-name = Default name
model-field-number = Number
model-field-content = Content
model-field-text-style = Text style
model-field-text-size = Size (mm)
model-field-anchor-position = Anchor position
model-field-angle = Angle
model-field-x = X
model-field-y = Y
model-predefined-type-none = None
model-position-undefined = Undefined
model-position-exterior = Exterior
model-position-interior = Interior
model-top-mode-fixed = Fixed height
model-top-mode-storey = Linked to storey
model-top-mode-relative = Home storey + n
model-top-linked-storey = Home { $storeys }
model-slab-reference-top = Top face
model-slab-reference-bottom = Bottom face
model-anchor-top-left = Top left
model-anchor-top-center = Top centre
model-anchor-top-right = Top right
model-anchor-middle-left = Middle left
model-anchor-center = Centre
model-anchor-middle-right = Middle right
model-anchor-bottom-left = Bottom left
model-anchor-bottom-center = Bottom centre
model-anchor-bottom-right = Bottom right
