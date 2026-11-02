import pite


class Player(pite.Node2D):
    hit = pite.signal(int)
    speed: float = 200.0

    def _ready(self):
        self.position = (160.0, 240.0)

    def _process(self, delta: float):
        if pite.held.right:
            self.position.x += self.speed * delta
        if pite.held.left:
            self.position.x -= self.speed * delta
        if pite.held.down:
            self.position.y += self.speed * delta
        if pite.held.up:
            self.position.y -= self.speed * delta
        if pite.pressed.space:
            self.position.x += 10.0
            self.hit.emit(1)
