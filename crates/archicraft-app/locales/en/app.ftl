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

# The submenu after File > Open… with the recently opened project files.
menu-file-recent = Open recent
menu-file-recent-empty = No recent projects

## Window and tab titles (UI-001)

# $name: the project's file name, or its name while it was never saved.
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
# $name: the project's file name, or its name while it was never saved.
modal-unsaved-text = “{ $name }” has unsaved changes. Save them before continuing?
modal-load-report-title = The project was opened with problems
modal-load-report-text = Everything that could be read was loaded. These objects were skipped, repaired or replaced:
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

## Status bar (UI-001): the prompt while no tool step is in progress, the
## problem count and the view-control bar of the active view

status-prompt-select = Click an element to select it, Shift-click to add or remove, drag a rectangle to select several · Tab cycles overlapping elements
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
status-fit = Fit
status-orbit = Orbit
# The menu of the 3D view presets.
status-views = Views
status-perspective = Perspective
status-perspective-hint = Switch between perspective and parallel projection (VIEW-020)
status-next-zoom = Next zoom ▶
status-previous-zoom = ◀ Previous zoom
# The cursor in a plan from the user origin (INP-014); $x, $y: formatted
# lengths with their unit.
status-cursor = X { $x }  Y { $y }
# The plan's zoom; $scale: points per metre in the user's number format.
status-zoom = { $scale } pt/m
status-scale = Scale 1:100
# $storey: the name of the plan's storey.
status-plan = Plan: { $storey }
# The indicator while groups are suspended (ELM-015); a click turns the
# suspension off.
status-groups-suspended = Groups suspended

## Toolbox (UI-001): group headings and the short labels of its tools; the
## tooltips are the commands' (commands.ftl)

toolbox-selection = Selection
toolbox-modelling = Modelling
toolbox-view-tools = View tools
toolbox-select = Select
toolbox-wall = Wall
toolbox-slab = Slab
toolbox-fit = Fit
toolbox-orbit = Orbit
toolbox-plan-3d = Plan/3D

## Project browser (VIEW-003, STR-003, UI-005)

browser-storeys = Storeys
browser-views = 3D views
browser-project = Project
# A storey entry; $number: the storey number, padded to three characters;
# $name: the storey's name; $elevation: its elevation with the unit.
browser-storey = { $number }  { $name }  ({ $elevation })
browser-open-plan = Open plan
browser-insert-above = Insert storey above
browser-insert-below = Insert storey below
browser-storey-settings = Rename, elevation, height…
browser-delete-storey = Delete storey…
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
browser-layers = Layers…
browser-materials = Materials…
browser-settings = Settings…
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
# Shown in a 3D view when the computer has no usable graphics adapter.
view-3d-no-gpu = 3D views need a GPU (wgpu), which is not available.
# The corner label of a 3D view; $projection: view-projection-perspective or
# view-projection-parallel; $mode: the display mode, for example Shaded.
view-3d-overlay = { $projection } · { $mode }
# The same while orbit mode is on.
view-3d-overlay-orbit = { $projection } · { $mode } · Orbit
view-projection-perspective = Perspective
view-projection-parallel = Parallel

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
