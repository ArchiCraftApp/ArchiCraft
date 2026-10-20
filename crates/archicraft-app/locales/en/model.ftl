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
