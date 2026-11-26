import pite


class Player(pite.CharacterBody2D):
    speed: float = 350.0

    def _process(self, delta: float):
        # Gather directional input
        move_x, move_y = pite.action.vector("left", "right", "up", "down")

        # Apply movement scaled by frame delta
        self.position.x += move_x * self.speed * delta
        self.position.y += move_y * self.speed * delta

        # Keep player within screen bounds
        self.position.x = max(30.0, min(1250.0, self.position.x))
        self.position.y = max(30.0, min(690.0, self.position.y))
