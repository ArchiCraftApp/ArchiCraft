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
