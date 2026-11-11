### ArchiCraft English message catalogue: commands (NFR-L10N-001, UI-008).
###
### One label and one description per command, looked up with keys built at
### runtime from the command id (crates/archicraft-app/src/commands):
### cmd-<command-id>-label and cmd-<command-id>-desc, with the dots of the id
### written as hyphens (file.save-as becomes cmd-file-save-as-label).

## File

cmd-file-new-label = New project
cmd-file-new-desc = Start a new project from the default template.
cmd-file-open-label = Open…
cmd-file-open-desc = Open a project file.
cmd-file-save-label = Save
cmd-file-save-desc = Save the project to its file.
cmd-file-save-as-label = Save as…
cmd-file-save-as-desc = Save the project to a new file.
cmd-file-close-label = Close project
cmd-file-close-desc = Close the project and start with an empty one.
cmd-file-preferences-label = Preferences…
cmd-file-preferences-desc = Change the settings that apply to every project.
cmd-file-quit-label = Quit
cmd-file-quit-desc = Close ArchiCraft.

## Edit

cmd-edit-undo-label = Undo
cmd-edit-undo-desc = Take back the last change to the model.
cmd-edit-redo-label = Redo
cmd-edit-redo-desc = Repeat the last change that was undone.
# The Undo and Redo menu items with the label of the change, such as
# "Undo Create wall"; $action is the change's label.
cmd-edit-undo-named = Undo { $action }
cmd-edit-redo-named = Redo { $action }
cmd-edit-delete-label = Delete
cmd-edit-delete-desc = Delete the selected elements.
cmd-edit-select-all-label = Select all
cmd-edit-select-all-desc = Select all visible elements of the kind the active tool draws, on the storey of the active plan.
cmd-edit-group-label = Group
cmd-edit-group-desc = Group the selected elements and groups, so that a click on one member selects them all.
cmd-edit-ungroup-label = Ungroup
cmd-edit-ungroup-desc = Remove the outermost level of grouping from the selected groups.
cmd-edit-suspend-groups-label = Suspend groups
cmd-edit-suspend-groups-desc = Select single members of groups with a click, for this session.
cmd-edit-settings-label = Settings of tool or selection…
cmd-edit-settings-desc = Edit the settings of the selected elements, or the defaults of the active tool.
cmd-edit-pick-up-parameters-label = Pick up parameters
cmd-edit-pick-up-parameters-desc = Pick up the settings of the elements you click into the active tool's defaults, until Esc.
cmd-edit-inject-parameters-label = Inject parameters
cmd-edit-inject-parameters-desc = Inject the active tool's settings into the elements you click, or into the selection, until Esc.
# The shortcut of a command that a click with modifier keys held also runs,
# such as Alt-click; $modifiers: the keys as the platform writes them.
cmd-gesture-click = { $modifiers }-click

## View

cmd-view-zoom-in-label = Zoom in
cmd-view-zoom-in-desc = Magnify the active view about its centre.
cmd-view-zoom-out-label = Zoom out
cmd-view-zoom-out-desc = Shrink the active view about its centre.
cmd-view-fit-window-label = Fit to window
cmd-view-fit-window-desc = Zoom the active view so that the whole model fits.
cmd-view-zoom-selection-label = Zoom to selection
cmd-view-zoom-selection-desc = Zoom the active view to the selected elements.
cmd-view-previous-zoom-label = Previous zoom
cmd-view-previous-zoom-desc = Go back to the previous zoom of the plan.
cmd-view-next-zoom-label = Next zoom
cmd-view-next-zoom-desc = Go forward to the next zoom of the plan.
cmd-view-switch-plan-3d-label = Switch between plan and 3D
cmd-view-switch-plan-3d-desc = Switch between the plan of the active storey and the 3D view.
cmd-view-next-tab-label = Next tab
cmd-view-next-tab-desc = Show the next open view.
cmd-view-previous-tab-label = Previous tab
cmd-view-previous-tab-desc = Show the previous open view.
cmd-view-orbit-label = Orbit mode
cmd-view-orbit-desc = Turn orbiting with the left mouse button in the 3D view on or off.
cmd-view-reference-lines-label = Show wall reference lines
cmd-view-reference-lines-desc = Show or hide the reference lines of walls in the plan.
cmd-view-problems-label = Problems
cmd-view-problems-desc = Show or hide the list of problems in the model and the loaded file.
cmd-view-place-user-origin-label = Place user origin
cmd-view-place-user-origin-desc = Click an origin and the direction of its X axis to show and type plan coordinates from them.
cmd-view-reset-user-origin-label = Reset user origin
cmd-view-reset-user-origin-desc = Measure coordinates from the project origin again.

## View: the active 3D view (VIEW-020, VIEW-050)

cmd-view3d-display-shaded-label = Shaded
cmd-view3d-display-shaded-desc = Show the active 3D view with shaded faces.
cmd-view3d-display-shaded-with-edges-label = Shaded with edges
cmd-view3d-display-shaded-with-edges-desc = Show the active 3D view with shaded faces and their edges.
cmd-view3d-display-hidden-line-label = Hidden line
cmd-view3d-display-hidden-line-desc = Show only the visible edges of the active 3D view.
cmd-view3d-display-wireframe-label = Wireframe
cmd-view3d-display-wireframe-desc = Show all edges of the active 3D view, also the hidden ones.
cmd-view3d-preset-top-label = Top
cmd-view3d-preset-top-desc = Look straight down on the model in the active 3D view.
cmd-view3d-preset-front-label = Front
cmd-view3d-preset-front-desc = Look at the front of the model in the active 3D view.
cmd-view3d-preset-side-label = Side
cmd-view3d-preset-side-desc = Look at the side of the model in the active 3D view.
cmd-view3d-preset-isometric-label = Isometric
cmd-view3d-preset-isometric-desc = Look at the model from above at the isometric angle in the active 3D view.
cmd-view3d-preset-dimetric-label = Dimetric
cmd-view3d-preset-dimetric-desc = Look at the model from above at the dimetric angle in the active 3D view.
cmd-view3d-toggle-projection-label = Perspective
cmd-view3d-toggle-projection-desc = Switch the active 3D view between perspective and parallel projection.

## Design

cmd-tool-select-label = Selection tool
cmd-tool-select-desc = Select elements by clicking them or dragging a rectangle.
cmd-tool-wall-label = Wall tool
cmd-tool-wall-desc = Draw walls along their reference line in a length you choose. Thickness, height and material can be changed later.
cmd-tool-slab-label = Slab tool
cmd-tool-slab-desc = Draw slabs by the corners of their outline. Thickness, elevation and material can be changed later.
cmd-tool-space-label = Space tool
cmd-tool-space-desc = Create spaces by clicking inside a room bounded by walls, or draw their boundary.
cmd-tool-text-label = Text tool
cmd-tool-text-desc = Place a text at a clicked anchor and type it in place. Double-click a text to edit it.
cmd-tool-measure-label = Measure tool
cmd-tool-measure-desc = Measure distances along picked points and angles in working units, without changing the model.
cmd-tool-wall-mode-single-label = Single
cmd-tool-wall-mode-single-desc = Draw one wall from its start to its end point.
cmd-tool-wall-mode-chain-label = Chain
cmd-tool-wall-mode-chain-desc = Draw connected walls point by point until the chain is finished.
cmd-tool-wall-mode-rectangle-label = Rectangle
cmd-tool-wall-mode-rectangle-desc = Draw four walls around a rectangle from two opposite corners.
cmd-tool-wall-mode-rotated-rectangle-label = Rotated rectangle
cmd-tool-wall-mode-rotated-rectangle-desc = Draw four walls around a rectangle from one side and its depth.
cmd-tool-slab-mode-polygon-label = Polygon
cmd-tool-slab-mode-polygon-desc = Draw a slab by the corners of its outline.
cmd-tool-slab-mode-rectangle-label = Rectangle
cmd-tool-slab-mode-rectangle-desc = Draw a rectangular slab from two opposite corners.
cmd-tool-slab-mode-rotated-rectangle-label = Rotated rectangle
cmd-tool-slab-mode-rotated-rectangle-desc = Draw a rectangular slab from one side and its depth.
cmd-tool-space-mode-detect-label = Detect
cmd-tool-space-mode-detect-desc = Create a space on the room around a click, bounded by walls.
cmd-tool-space-mode-polygon-label = Polygon
cmd-tool-space-mode-polygon-desc = Draw a space by the corners of its boundary.
cmd-tool-space-mode-rectangle-label = Rectangle
cmd-tool-space-mode-rectangle-desc = Draw a rectangular space from two opposite corners.
cmd-tool-contour-detection-label = Contour detection
cmd-tool-contour-detection-desc = Switch contour detection on or off: a wall, slab or space click then traces the clicked edge, chain, region or room.

## Project

cmd-project-storeys-label = Storeys…
cmd-project-storeys-desc = Add, rename, reorder and delete the storeys of the project.
cmd-project-layers-label = Layers…
cmd-project-layers-desc = Add, rename and delete layers, and show, hide or lock them.
cmd-project-materials-label = Materials…
cmd-project-materials-desc = Add and edit the materials of the project.
cmd-project-settings-label = Project settings…
cmd-project-settings-desc = Change the project's name, address, units and other settings.
cmd-storey-insert-above-label = Insert storey above
cmd-storey-insert-above-desc = Insert a new storey above the storey.
cmd-storey-insert-below-label = Insert storey below
cmd-storey-insert-below-desc = Insert a new storey below the storey.
cmd-storey-edit-label = Rename, elevation, height…
cmd-storey-edit-desc = Open the Storeys dialog to change the storey's name, elevation and height.
cmd-storey-delete-label = Delete storey…
cmd-storey-delete-desc = Delete the storey after asking what happens to its elements.
cmd-material-duplicate-label = Duplicate material
cmd-material-duplicate-desc = Add a copy of the material selected in the Materials dialog.

## Help

cmd-help-shortcuts-label = Keyboard shortcuts
cmd-help-shortcuts-desc = List the keyboard shortcuts.
cmd-help-about-label = About ArchiCraft
cmd-help-about-desc = Show the version, copyright, website and licence of ArchiCraft.

## Keys of the plan view (local commands)

cmd-tool-wall-cycle-reference-line-label = Cycle wall reference line
cmd-tool-wall-cycle-reference-line-desc = Move the reference line of the wall tool to its next location.
cmd-canvas-cycle-candidates-label = Cycle overlapping elements
cmd-canvas-cycle-candidates-desc = Highlight the next of the elements under the cursor; the next click selects it.
cmd-input-next-field-label = Next typed field
cmd-input-next-field-desc = Move to the next typed field, or to the next snap candidate while several are under the cursor.
cmd-input-remove-last-label = Remove last point
cmd-input-remove-last-desc = Remove the last typed character, or else the last point.
cmd-input-finish-label = Finish input
cmd-input-finish-desc = Take the typed point, or end the chain.
cmd-input-cancel-step-label = Cancel step
cmd-input-cancel-step-desc = Cancel one step: the typed text, the last point, the selection, then the tool.
cmd-input-hold-contour-detection-label = Hold contour detection
cmd-input-hold-contour-detection-desc = While held, a wall, slab or space click traces the clicked edge, chain, region or room.

## Reasons a command is disabled (Enabled::No in the command registry)

cmd-reason-nothing-to-undo = There is nothing to undo.
cmd-reason-nothing-to-redo = There is nothing to redo.
cmd-reason-nothing-selected = Nothing is selected.
cmd-reason-no-user-origin = No user origin is placed.
cmd-reason-nothing-to-group = Select at least two elements or groups to group.
cmd-reason-nothing-grouped = No selected element belongs to a group.
cmd-reason-no-3d-view = No 3D view is active.
cmd-reason-no-material-selected = No material is selected in the Materials dialog.
