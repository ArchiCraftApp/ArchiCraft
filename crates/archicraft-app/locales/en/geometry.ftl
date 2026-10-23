### ArchiCraft English message catalogue: geometry problems
### (NFR-L10N-001, UI-012).
###
### archicraft-geometry has no catalogue of its own: every element whose
### geometry fails is listed in the problems panel with a message key
### starting with geometry-, which the application looks up here. The
### panel shows each as "<element name>: <message>". $reason is another
### message (a geometry-polygon- or model-error- key) rendered in place.

## Why an element has no body.

geometry-invalid-polygon = the outline is invalid: { $reason }
geometry-no-height = the element has no height
geometry-home-storey-missing = the home storey is missing
geometry-linked-storey-missing = the linked storey is missing
# $kind is an element kind noun, such as wall.
geometry-kind-unsupported = elements of the kind “{ $kind }” have no geometry yet
geometry-evaluation-panicked = geometry evaluation failed unexpectedly

## Why the triangulation refused an outline.

geometry-polygon-too-few-points = it has fewer than three points
geometry-polygon-not-finite = a coordinate is not a finite number
geometry-polygon-coordinate-range = a coordinate is too large or too small
geometry-polygon-duplicate-point = a point occurs twice
geometry-polygon-self-intersection = it intersects itself
geometry-polygon-no-area = it has no area
