### ArchiCraft English message catalogue: main window (NFR-L10N-001).
###
### Menus, window and tab titles, questions and notices, the status line, the
### toolbox and the project browser, the plan and 3D views. Keys are
### kebab-case and start with menu-, window-, tab-, modal-, file-, startup-,
### status-, toolbox-, browser-, view-, display-mode-, view-preset- or
### snap-kind- (see crates/archicraft-app/src/i18n).

## Menu bar (UI-011)
## The titles of the menus; the items are commands (commands.ftl).

menu-file = File
menu-edit = Edit
menu-view = View
menu-design = Design
menu-project = Project
menu-help = Help
# The submenus of the geometry methods (Design) and the 3D views (View).
menu-design-wall-mode = Wall method
menu-design-slab-mode = Slab method
menu-design-space-mode = Space method
menu-design-column-mode = Column method
menu-view-display-mode = Display mode
menu-view-view-preset = View presets

# The submenu after File > Open… with the recently opened project files.
menu-file-recent = Open recent
menu-file-recent-empty = No recent projects

## Window and tab titles (UI-001)

# $name: the project's name (Project settings).
window-title = { $name } — ArchiCraft
# The same while the project has unsaved changes.
window-title-unsaved = ● { $name } — ArchiCraft
# $storey: the storey's name, for example 00 Ground floor.
tab-plan = Plan: { $storey }
# A plan tab whose storey no longer exists, for the frame before it closes.
tab-plan-missing = Plan
# A 3D tab whose view no longer exists, for the frame before it closes.
tab-3d-missing = 3D
tab-none = Open a plan or 3D view from the project browser.

## Questions and messages in front of the main window (PRJ-003, PRJ-008,
## IFX-062)

modal-ok = OK
modal-cancel = Cancel
modal-save = Save
modal-dont-save = Don't save
modal-unsaved-title = Save changes?
# $name: the project's name; for a project that was never saved.
modal-unsaved-text = “{ $name }” has unsaved changes. Save them before continuing?
# $name: the project's name; $file: the file it was saved to.
modal-unsaved-text-file = “{ $name }” ({ $file }) has unsaved changes. Save them before continuing?
modal-load-report-title = The project was opened with problems
modal-load-report-text = Everything that could be read was loaded. These objects were skipped, repaired or replaced:
# $from: the ArchiCraft format the file was written in; $to: the format it
# was converted to, the one this version saves.
modal-load-report-migrated = The file was written in ArchiCraft format { $from } by an earlier version and was converted to format { $to }. Saving stores it in format { $to }.
modal-newer-title = Overwrite a newer file?
modal-newer-text = This file was written by a newer version of ArchiCraft. Saving it with this version may lose data that this version does not know.
modal-save-anyway = Save anyway
modal-cannot-open-title = Cannot open the project
# $path: the file; $reason: why it cannot be opened.
modal-cannot-open-text = { $path }: { $reason }
modal-not-saved-title = The project was not saved
# $path: the file; $reason: the system's error message.
modal-not-saved-text = { $path }: { $reason }
modal-recovery-failed-title = Recovery failed

# The file type in the Open and Save dialogs.
file-filter-ifcx = IFCX project

## Startup notices: autosaves to recover (PRJ-004) and the crash report of
## the last session (NFR-SEC-002)

startup-recovery-title = Recover unsaved work
startup-recovery-text = ArchiCraft did not close properly last time. These autosaves can be recovered (PRJ-004):
# $name: the project's name; $time: when it was autosaved; $place: its
# file, or startup-recovery-never-saved.
startup-recovery-entry = { $name } — autosaved { $time } ({ $place })
startup-recovery-never-saved = never saved
startup-recovery-recover = Recover
startup-recovery-discard = Discard
startup-recovery-later = Decide later
startup-crash-title = ArchiCraft crashed last time
startup-crash-text = A crash report was saved on this computer. Nothing was sent. You can pass it on to the developers if you want to:
startup-crash-copy-path = Copy path

## Status line messages

# $path: the file.
status-opened = Opened { $path }
# $path: the file; $from: the ArchiCraft format it was written in; $to: the
# format it was converted to.
status-opened-migrated = Opened { $path }, converted from ArchiCraft format { $from } to format { $to }
# $path: the file.
status-saved = Saved { $path }
# $warning: the first warning of the save.
status-saved-with-warnings = Saved with warnings: { $warning }
status-recovered = Recovered the autosaved project. Save it to keep it.
# $command: what was undone, for example Create wall.
status-undone = Undone: { $command }
# $command: what was redone, for example Create wall.
status-redone = Redone: { $command }
status-nothing-to-undo = Nothing to undo
status-nothing-to-redo = Nothing to redo
# $command: the rejected command, for example Create wall; $reason: why the
# project rejected it.
status-rejected = { $command }: { $reason }
status-user-origin-reset = User origin reset to the project origin
status-user-origin-placed = User origin placed
# A double-click in a 3D view moved the orbit pivot to the clicked point.
status-orbit-pivot-set = Orbit pivot set
# $location: where the wall tool's reference line now lies, for example
# Exterior face (WAL-007).
status-wall-reference-line = Wall reference line: { $location }
status-selection-locked = The selected elements are locked.
# $count: the locked elements of the selection that stayed, for the plural;
# $count_text: the same number in the user's number format.
status-locked-not-deleted =
    { $count ->
        [one] { $count_text } locked element was not deleted.
       *[other] { $count_text } locked elements were not deleted.
    }
# $count: the elements now selected, for the plural; $count_text: the same
# number in the user's number format.
status-selected =
    { $count ->
        [one] { $count_text } element selected
       *[other] { $count_text } elements selected
    }

## Parameter transfer (ELM-003): picking an element's settings up into a
## tool's defaults and injecting a tool's defaults into elements

# $count: the settings picked up, for the plural; $count_text: the same
# number in the user's number format; $name: the element's name, such as
# W-003.
status-picked-up =
    { $count ->
        [one] Picked up { $count_text } setting from { $name }
       *[other] Picked up { $count_text } settings from { $name }
    }
# $name: the element whose settings the tool's defaults already had.
status-picked-up-unchanged = The tool's defaults already have the settings of { $name }
# $total: the source's settings, for the plural; $applied_text: the settings
# the element took and $total_text: the source's settings, both in the
# user's number format; $name: the element's name, such as SL-002.
status-injected =
    { $total ->
        [one] Injected { $applied_text } of { $total_text } setting into { $name }
       *[other] Injected { $applied_text } of { $total_text } settings into { $name }
    }
# $injected: the message status-injected; $skipped: the settings the element
# did not take, each a status-transfer-skip- message, separated by commas.
status-injected-skipped = { $injected } ({ $skipped })
# $count: the elements, for the plural; $count_text: the same number in the
# user's number format.
status-injected-many =
    { $count ->
        [one] Injected the settings into { $count_text } element
       *[other] Injected the settings into { $count_text } elements
    }
# $name: the element that had the settings already.
status-injected-unchanged = { $name } already has these settings
# $count: the elements, for the plural; $count_text: the same number in the
# user's number format.
status-injected-unchanged-many =
    { $count ->
        [one] { $count_text } element already has these settings
       *[other] { $count_text } elements already have these settings
    }
# $injected: one of the status-injected messages; $count: the locked
# elements of the selection, for the plural; $count_text: the same number in
# the user's number format.
status-injected-locked =
    { $count ->
        [one] { $injected }; { $count_text } locked element was skipped
       *[other] { $injected }; { $count_text } locked elements were skipped
    }
# Inject with the selection or the measure tool active: no tool defaults to
# inject.
status-inject-no-defaults = Choose the tool whose settings to inject
# $field: the label of a setting the target did not take, such as IFC class.
status-transfer-skip-not-offered = { $field } not allowed
# $field: the label of a setting that does not apply to the target as it is.
status-transfer-skip-inapplicable = { $field } not applicable
# $field: the label of a setting whose value the target rejects; $reason:
# why.
status-transfer-skip-invalid = { $field }: { $reason }

## The preview of a parameter transfer while hovering an element (ELM-003,
## EDT-020)

# $name: the hovered element's name, such as W-003.
view-transfer-preview-pick-up = Pick up from { $name }
view-transfer-preview-inject = Inject into { $name }
# $count: the selected elements, for the plural; $count_text: the same
# number in the user's number format; $name: the hovered one's name.
view-transfer-preview-inject-selection =
    { $count ->
        [one] Inject into { $count_text } selected element, such as { $name }
       *[other] Inject into { $count_text } selected elements, such as { $name }
    }
# $fields: the labels of the settings a click would transfer, separated by
# commas.
view-transfer-preview-applied = Transfers: { $fields }
# $fields: the settings a click would skip, each a status-transfer-skip-
# message, separated by commas.
view-transfer-preview-skipped = Skips: { $fields }
view-transfer-preview-nothing = Nothing to transfer
# The hovered element is locked, so an inject would be refused (ELM-013).
view-transfer-preview-locked = Locked: nothing would change

## Status bar (UI-001): the prompt while no tool step is in progress, the
## problem count and the cursor in a plan

status-prompt-select = Click an element to select it, Shift-click to add or remove, drag a rectangle to select several · Tab cycles overlapping elements
# While Pick up parameters or Inject parameters is latched (ELM-003).
status-prompt-pick-up = Pick up: click an element to make its settings the tool's defaults · Esc ends
status-prompt-inject = Inject: click an element to give it the tool's settings · Esc ends
# Appended to a tool's prompt in press-drag-release input mode (INP-020).
status-prompt-press-drag-release = Press-drag-release: press at a point, drag and release at the next
# The button that shows the problems panel; $count: the problems, for the
# plural; $count_text: the same number in the user's number format.
status-problems =
    { $count ->
        [0] No problems
        [one] ⚠ { $count_text } problem
       *[other] ⚠ { $count_text } problems
    }
# The cursor in a plan from the user origin (INP-014); $x, $y: formatted
# lengths with their unit.
status-cursor = X { $x }  Y { $y }
# $storey: the name of the plan's storey.
status-plan = Plan: { $storey }
# The indicator while groups are suspended (ELM-015); a click turns the
# suspension off.
status-groups-suspended = Groups suspended
status-contour-detection = Contour detection on

## View-control bar (UI-001): the controls of the active view; the tooltips
## are the commands' (commands.ftl)

view-bar-zoom-in = Zoom in
view-bar-zoom-out = Zoom out
# The view's zoom; $percent: the number in the user's number format. In a
# plan 100 % is paper size at the drawing scale, in 3D the fitted view.
view-bar-zoom = { $percent } %
view-bar-zoom-hint = The zoom of the view. Click to type a percentage, Enter applies it, Esc keeps the zoom.
# A typed zoom that is not a number; $text: what was typed.
view-bar-zoom-not-a-number = “{ $text }” is not a zoom. Type a percentage such as 150.
# A typed zoom outside the view's limits; $min and $max in percent.
view-bar-zoom-out-of-range = Type a zoom between { $min } % and { $max } %.
view-bar-previous-zoom = ◀ Previous zoom
view-bar-next-zoom = Next zoom ▶
view-bar-fit = Fit
# The drawing scale of a 2D view (VIEW-013); $denominator: the n of 1:n.
view-bar-drawing-scale = Drawing scale
view-bar-drawing-scale-value = 1:{ $denominator }
view-bar-drawing-scale-custom = Custom…
view-bar-drawing-scale-hint = The scale of the drawing: line weights, hatches and text sizes are paper millimetres at this scale. Zooming does not change it.
view-bar-drawing-scale-custom-hint = Type a scale such as 1:75 or 75 and press Enter.
view-bar-orbit = Orbit
view-bar-reference-lines = Reference lines
# The menu of the 3D view presets.
view-bar-views = Views
view-bar-perspective = Perspective
# The icon button that saves the viewpoint as a camera view (VIEW-004).
view-bar-save-camera = Save camera
view-bar-display-mode-hint = How the active 3D view draws the model.
view-bar-view-preset-hint = Turn the camera of the active 3D view to a standard direction.

## Toolbox (UI-001): group headings and the short labels of its tools; the
## tooltips are the commands' (commands.ftl)

toolbox-selection = Selection
toolbox-modelling = Modelling
toolbox-annotation-2d = 2D and annotation
toolbox-view-tools = View tools
toolbox-select = Select
toolbox-wall = Wall
toolbox-slab = Slab
toolbox-space = Space
toolbox-column = Column
toolbox-text = Text
toolbox-fit = Fit
toolbox-orbit = Orbit
toolbox-plan-3d = Plan/3D
toolbox-measure = Measure

## Project browser (VIEW-003, STR-003, UI-005)

browser-storeys = Storeys
browser-views = 3D views
browser-project = Project
# A storey entry; $number: the storey number, padded to three characters;
# $name: the storey's name; $elevation: its elevation with the unit.
browser-storey = { $number }  { $name }  ({ $elevation })
browser-open-plan = Open plan
# $name: the site's name.
browser-site = Site: { $name }
# $name: the building's name.
browser-building = Building: { $name }
# $count: the elements of the project, for the plural; $count_text: the same
# number in the user's number format.
browser-elements =
    { $count ->
        [one] { $count_text } element
       *[other] { $count_text } elements
    }
# The name of a storey inserted at or above the ground storey; $number: its
# storey number with two digits, such as 02.
browser-new-storey = { $number } Storey
# The name of a storey inserted below the ground storey; $number: the depth
# below ground with two digits, such as 01 for storey -1.
browser-new-basement = -{ $number } Basement

## Plan and 3D views

# Beside the element under the cursor in a plan; $name: the element's name,
# for example W-1; $kind: its kind, for example Wall.
view-plan-hover = { $name } · { $kind }
# The accessible name of the inline editor of a text in a plan (ANN-007).
view-plan-text-editor = Text
# Shown in a 3D view when the computer has no usable graphics adapter.
view-3d-no-gpu = 3D views need a GPU (wgpu), which is not available.
# The corner label of a 3D view; $projection: view-projection-perspective or
# view-projection-parallel; $mode: the display mode, for example Shaded.
view-3d-overlay = { $projection } · { $mode }
# The same while orbit mode is on.
view-3d-overlay-orbit = { $projection } · { $mode } · Orbit
view-projection-perspective = Perspective
view-projection-parallel = Parallel
# The word a saved camera view is named by, followed by its number: Camera 1
# (VIEW-004).
view-camera-name = Camera

## Display modes of the 3D views (VIEW-050), from DisplayMode::label_key

display-mode-shaded = Shaded
display-mode-shaded-with-edges = Shaded with edges
display-mode-hidden-line = Hidden line
display-mode-wireframe = Wireframe

## View presets of the 3D views, from ViewPreset::label_key

view-preset-top = Top
view-preset-front = Front
view-preset-side = Side
view-preset-isometric = Isometric
view-preset-dimetric = Dimetric

# Tooltips of the display-mode choice and the Views menu in the status bar of
# a 3D view (UI-011).

## Snap kinds named in the cursor label (INP-008), from SnapKind::label_key

snap-kind-node = node
snap-kind-handle = handle
snap-kind-centre = centre
snap-kind-axis-point = axis point
snap-kind-centroid = centroid
snap-kind-intersection = intersection
snap-kind-midpoint = midpoint
snap-kind-division = division point
snap-kind-guide-intersection = guide intersection
snap-kind-perpendicular = perpendicular
snap-kind-tangent = tangent
snap-kind-edge = on edge
snap-kind-guide = guide
snap-kind-grid = grid
snap-kind-constrained = constrained
snap-kind-typed = typed
