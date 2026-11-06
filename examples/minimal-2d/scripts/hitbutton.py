import pite


class HitButton(pite.Button):
    def _ready(self):
        self.pressed.connect(self.on_pressed)

    def on_pressed(self):
        self.get_node("../Player").hit.emit(1)
