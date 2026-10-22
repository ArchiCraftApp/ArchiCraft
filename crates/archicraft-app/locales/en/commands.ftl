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
cmd-edit-delete-label = Delete
cmd-edit-delete-desc = Delete the selected elements.
cmd-edit-select-all-label = Select all
cmd-edit-select-all-desc = Select all visible elements of the kind the active tool draws, on the storey of the active plan.
cmd-edit-settings-label = Settings of tool or selection…
cmd-edit-settings-desc = Edit the settings of the selected elements, or the defaults of the active tool.

## View

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

## Design

cmd-tool-select-label = Selection tool
cmd-tool-select-desc = Select elements by clicking them or dragging a rectangle.
cmd-tool-wall-label = Wall tool
cmd-tool-wall-desc = Draw walls by their reference line.
cmd-tool-slab-label = Slab tool
cmd-tool-slab-desc = Draw slabs by their outline.

## Project

cmd-project-storeys-label = Storeys…
cmd-project-storeys-desc = Add, rename, reorder and delete the storeys of the project.
cmd-project-layers-label = Layers…
cmd-project-layers-desc = Add, rename and delete layers, and show, hide or lock them.
cmd-project-materials-label = Materials…
cmd-project-materials-desc = Add and edit the materials of the project.
cmd-project-settings-label = Project settings…
cmd-project-settings-desc = Change the project's name, address, units and other settings.

## Help

cmd-help-shortcuts-label = Keyboard shortcuts
cmd-help-shortcuts-desc = List the keyboard shortcuts.
cmd-help-about-label = About ArchiCraft
cmd-help-about-desc = Show the version of ArchiCraft and the file format it writes.
