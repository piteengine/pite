import pite


class HitButton(pite.Button):
    pressed = pite.signal()

    def _ready(self):
        self.get_node(".").pressed.connect(self.on_pressed)

    def on_pressed(self):
        self.get_node("../Player").hit.emit(1)
