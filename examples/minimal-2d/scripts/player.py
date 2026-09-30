import pite


class Player(pite.Node2D):
    hit = pite.signal(int)
    speed: float = 200.0

    def _ready(self):
        self.position = (100.0, 200.0)

    def _process(self, delta: float):
        x, y = self.position
        if pite.held("ArrowRight"):
            x += self.speed * delta
        if pite.held("ArrowLeft"):
            x -= self.speed * delta
        if pite.held("ArrowDown"):
            y += self.speed * delta
        if pite.held("ArrowUp"):
            y -= self.speed * delta
        if pite.pressed("Space"):
            x += 10.0
            self.hit.emit(1)
        self.position = (x, y)
