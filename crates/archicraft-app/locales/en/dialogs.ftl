### ArchiCraft English message catalogue: dialogs (NFR-L10N-001).
###
### Keys are kebab-case and start with dlg-<dialog>- (see
### crates/archicraft-app/src/i18n).

## About ArchiCraft

dlg-about-title = About ArchiCraft
# The alternative text of the app icon at the top of the window.
dlg-about-icon = ArchiCraft app icon
# $version: the application version as people read it, for example 0.1 or 0.1.2.
# "alpha" is the release stage; it changes with the releases.
dlg-about-heading = ArchiCraft { $version } (alpha)
dlg-about-summary = ArchiCraft is an open-source architectural CAD and BIM tool.
dlg-about-copyright = Copyright © 2026 Jakob Brandstötter and Alexander Partsch
# The licence notice of CC BY 4.0, the licence of LICENSE and README.
dlg-about-licence = ArchiCraft is open source, licensed under the Creative Commons Attribution 4.0 International licence (CC BY 4.0).
# The disclaimer of warranties of CC BY 4.0, section 5.
dlg-about-warranty = To the extent possible, ArchiCraft is provided as is and as available, without warranties of any kind (section 5 of the licence).
# $url: the address of the source code repository.
dlg-about-source = Source code: { $url }
# $version: the IFCX version written, for example ifcx_alpha.
dlg-about-format = Writes { $version } files.
dlg-about-privacy = Works offline. Sends no data anywhere; crash reports stay on this computer.

## Unit settings: Project settings and the default units of new projects (STR-009)

dlg-units-length-unit = Length unit
dlg-units-length-decimals = Length decimals
dlg-units-angle-unit = Angle unit
dlg-units-angle-decimals = Angle decimals
dlg-units-slope-format = Slope format
# $sample: a 30° pitch in the chosen slope format, for example 57.74 %.
dlg-units-slope-sample = 30° pitch: { $sample }
dlg-units-area-unit = Area unit
dlg-units-volume-unit = Volume unit

## Properties strip (UI-002)

dlg-properties-nothing-selected = Nothing selected. Choose a tool, or select elements to edit them.
# What the strip edits with nothing selected; $kind: the tool's element
# kind, such as Wall.
dlg-properties-tool-defaults = { $kind } tool defaults
# Several selected elements of different kinds; $count: their number, for
# the plural.
dlg-properties-elements =
    { $count ->
        [one] element
       *[other] elements
    }
# $count_text: the number of selected elements in the user's number format;
# $what: their kind's plural, such as walls, or elements.
dlg-properties-selected = { $count_text } { $what } selected
# As dlg-properties-selected; $editable_text: how many of them are not
# locked.
dlg-properties-selected-editable = { $count_text } { $what } selected, { $editable_text } editable
dlg-properties-all-locked = The selected elements are locked.
dlg-properties-name = Name
dlg-properties-home-storey = Home storey
dlg-properties-locked-toggle = Locked
dlg-properties-locked-toggle-hint = Locked elements reject moves, deletion and parameter changes (ELM-013)
dlg-properties-snap-targets = Snap targets
dlg-properties-snap-targets-hint = Cleared, the element stays visible but provides no snap targets (INP-017)
# Read-only values of one selected element (MEA-003, COL-001).
dlg-properties-length = Length
dlg-properties-area = Area
dlg-properties-section-area = Cross-section area
dlg-properties-height = Height
dlg-properties-volume = Volume
dlg-properties-settings = Settings…
dlg-properties-settings-hint = All settings (Ctrl/Cmd+T)
# The chevron at the right end of the strip that opens the fields that do
# not fit into the window.
dlg-properties-more = More properties

# A selection that is one group (ELM-015); $name: the group's name; $count:
# its elements, for the plural; $count_text: the same number in the user's
# number format.
dlg-properties-group =
    { $count ->
        [one] Group: { $name } ({ $count_text } element)
       *[other] Group: { $name } ({ $count_text } elements)
    }

# The text's style size in an empty Size field (ANN-007); $size: the size in
# millimetres.
dlg-properties-text-style-size = { $size } (style)
# A number field that takes a positive number (ANN-007).
dlg-properties-error-positive-number = Type a positive number.
# A number field that takes a whole number (UI-002).
dlg-properties-error-whole-number = Type a whole number.
