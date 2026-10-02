import pite


class HitButton(pite.Button):
    # No `pressed = pite.signal()` needed: `pressed` belongs to the
    # Button type itself and is declared by the engine.
    def _ready(self):
        self.get_node(".").pressed.connect(self.on_pressed)

    def on_pressed(self):
        self.get_node("../Player").hit.emit(1)
