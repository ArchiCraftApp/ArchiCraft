### ArchiCraft English message catalogue: reading and writing IFCX files
### (NFR-L10N-001, PRJ-008).
###
### archicraft-ifcx has no catalogue of its own: every problem it reports is
### a message key starting with ifcx-, which the application looks up here.
### Arguments named reason, record, change and message are other messages
### (ifcx- or model- keys) rendered in place; detail is a text of the JSON
### parser or the operating system, shown as it is.

## A problem with the node it was found at, as the load report lists it.

ifcx-problem-at = [{ $path }] { $message }

## Reading and validating a file (IFX-005, IFX-083, Appendix D.9).

ifcx-schema-unreadable = Schema “{ $id }” cannot be read: { $detail }
ifcx-import-offline = Import “{ $uri }” is not available offline; its attributes cannot be checked.
ifcx-schema-too-deep = schema nesting is too deep
ifcx-schema-unknown-inherited = unknown inherited schema “{ $id }”
ifcx-schema-array-restrictions = an Array schema needs arrayRestrictions
ifcx-schema-blob = the Blob data type is not supported
# $expected and $found are JSON type names, such as boolean or object.
ifcx-value-type = expected { $expected }, found { $found }
ifcx-value-not-option = “{ $value }” is not one of { $options }
ifcx-value-key-missing = missing key “{ $key }”
ifcx-value-in-key = { $key }: { $reason }
ifcx-attribute-undeclared = attribute “{ $id }” is not declared
ifcx-attribute-invalid = attribute “{ $id }”: { $reason }
ifcx-detail = { $detail }
ifcx-composition-too-deep = composition is nested too deeply
ifcx-composition-cycle = composition cycle
ifcx-unknown-node = refers to the unknown node “{ $target }”
ifcx-roots = the composed tree has { $count_text } roots instead of one
ifcx-file-too-large = the file is larger than the limit of { $limit }
ifcx-size-gb = { $size_text } GB
ifcx-size-mb = { $size_text } MB
ifcx-size-kb = { $size_text } KB
ifcx-size-bytes = { $size ->
        [one] { $size_text } byte
       *[other] { $size_text } bytes
    }
ifcx-file-unreadable = the file cannot be read: { $detail }
ifcx-file-nested-too-deep = the file is nested more than { $max_text } levels deep
ifcx-file-invalid = the file is not a valid IFCX document: { $detail }
ifcx-file-too-many-imports = the file has more than { $max_text } imports
ifcx-version-unsupported = the IFCX version “{ $version }” is not supported; this release reads { $supported }

## Migrating files of earlier ArchiCraft formats (IFX-062, IFX-091).

ifcx-migration-unknown = no migration from ArchiCraft format { $version_text } is known; the file is read as it is
ifcx-migration-failed = the file cannot be migrated from ArchiCraft format { $version_text }: { $detail }; it is read as it is
ifcx-migrated = migrated from format { $from_text } to format { $to_text }: { $change }

## The load report (PRJ-008): “<record> skipped: <reason>” and
## “<record> repaired: <reason>”.

ifcx-skipped = { $record } skipped: { $reason }
ifcx-repaired = { $record } repaired: { $reason }

## Records.

ifcx-record-project = project
ifcx-record-project-settings = project settings
ifcx-record-project-location = project location
ifcx-record-project-information = project information
ifcx-record-material = material record { $index_text }
ifcx-record-layer = layer record { $index_text }
ifcx-record-view = view record { $index_text }
ifcx-record-tool-defaults = tool defaults
ifcx-record-counters = counters
ifcx-record-site = site
ifcx-record-building = building
ifcx-record-storey = storey { $name }
ifcx-record-element = element { $name }

## Reasons, and what was used instead. $key is the name of a field in the
## file, such as thickness.

ifcx-default-used = { $reason }; a default value is used
ifcx-left-out = { $reason }; it is left out
ifcx-value-used = { $reason }; { $value_text } is used
ifcx-field-missing = “{ $key }” is missing
ifcx-field-not-number = “{ $key }” is not a number
ifcx-field-not-integer = “{ $key }” is not an integer
ifcx-field-not-text = “{ $key }” is not text
ifcx-field-not-boolean = “{ $key }” is not true or false
ifcx-field-not-object = “{ $key }” is not an object
ifcx-field-not-list = “{ $key }” is not a list
ifcx-field-unknown-value = “{ $key }” has the unknown value “{ $value }”
ifcx-field-not-uuid = “{ $key }” is not a UUID
ifcx-field-out-of-range = “{ $key }” is out of range
ifcx-field-point = “{ $key }”: { $reason }
ifcx-field-colour-component = “{ $key }” has a non-numeric component
ifcx-field-not-colour = “{ $key }” is not a colour
ifcx-point-not-list = a point is not a list
ifcx-point-not-numeric = a point has non-numeric coordinates
ifcx-point-coordinates = a point does not have two coordinates
ifcx-points-not-list = a point list is not a list
ifcx-not-object = it is not an object
ifcx-path-not-uuid = the path is not a UUID; the object gets a new identity
ifcx-no-name = it has no name; “{ $name }” is used
ifcx-material-corrected = “{ $name }”: { $reason }; the values are corrected
ifcx-grid-spacings = “auxiliarySpacing” is larger than “mainSpacing”; the default spacings are used
ifcx-out-of-range-replaced = “{ $key }” is out of range; { $value_text } is used
ifcx-newer-format = the file was written by a newer ArchiCraft version (format { $version_text }); data this version does not know may be lost on saving
ifcx-format-version-invalid = the ArchiCraft format version is missing or invalid
ifcx-ground-storey-repaired = { $reason }; the lowest storey at or above project zero is the ground storey
ifcx-template-defaults-used = { $reason }; the template defaults are used
# $counts lists the kind codes with their numbers of elements, such as “wall: 3, slab: 1”.
ifcx-counters-from-elements = { $reason }; the number of elements of each kind ({ $counts }) is used
ifcx-counter-negative = “{ $key }” is negative; { $value_text } is used
ifcx-tool-unknown = “{ $key }” is not a tool this version knows; it is ignored
ifcx-tool-template-used = { $reason }; the template defaults of the tool are used
ifcx-tool-material-missing = a tool's default material does not exist; the template default is used
ifcx-tool-layer-missing = a tool's default layer does not exist; the template default is used
ifcx-tool-class-not-allowed = { $class } is not an allowed class for the { $kind } tool; { $default } is used
ifcx-storey-listed-twice = the building lists it more than once
ifcx-storey-no-data = it has no ArchiCraft data; elevation { $elevation } and height { $height } are assumed
ifcx-storey-height-min = “height” is below the minimum; { $min } is used
ifcx-no-storeys = the building has no storeys; a ground floor was added
ifcx-ground-storey-undefined = the ground storey is not defined; the lowest storey at or above project zero is used
ifcx-storey-moved-up = storey “{ $name }” coincided with the one below and was moved up
ifcx-storeys-unreadable = the storeys could not be read ({ $reason }); one storey is used
ifcx-storey-lost = its storey “{ $name }” could not be read
ifcx-element-data-not-object = its ArchiCraft data is not an object
ifcx-element-layer-missing = the layer does not exist; the first layer is used
ifcx-element-class-unsupported = unsupported IFC class; { $default } is used
ifcx-predefined-type-not-offered = { $type } is not a predefined type of { $class }; it is left out
ifcx-element-material-missing = the material does not exist; “{ $name }” is used
ifcx-parameters-missing = { $kind } parameters are missing
ifcx-holes-left-out = the holes are left out: { $reason }
ifcx-kind-unreadable = { $kind } elements cannot be read yet
ifcx-no-project = this IFCX file holds no ArchiCraft project; opening IFCX files from other tools is not supported yet
ifcx-project-not-object = the ArchiCraft project data is not an object
ifcx-no-site = the project has no site; a site was added
ifcx-no-building = the project has no building; a building was added
ifcx-no-views = the project has no views; a default view was added
ifcx-no-materials = the project has no materials; a default material was added
ifcx-no-layers = the project has no layers; a default layer was added
ifcx-data-version-invalid = the data version in the header is not a number
ifcx-foreign-node = “{ $name }” has no ArchiCraft data and was not loaded; importing elements from other tools is not supported yet
ifcx-duplicate = { $name } appears twice; the second occurrence was skipped

## Saving (IFX-010, ELM-001).

ifcx-tool-defaults-not-saved = The defaults of the { $kind } tool cannot be stored in this file format yet and were not saved.
ifcx-saved-without-body = { $name } has no valid geometry and was saved without a body.
ifcx-kind-not-storable = { $name } ({ $kind }) cannot be stored in this file format yet; it was saved without its parameters and will not open as an editable element.
