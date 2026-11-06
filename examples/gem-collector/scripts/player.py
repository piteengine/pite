import pite


class Player(pite.CharacterBody2D):
    speed: float = 350.0

    def _process(self, delta: float):
        # Gather directional input
        move_x = 0.0
        move_y = 0.0

        if pite.held.right or pite.held.d:
            move_x += 1.0
        if pite.held.left or pite.held.a:
            move_x -= 1.0
        if pite.held.down or pite.held.s:
            move_y += 1.0
        if pite.held.up or pite.held.w:
            move_y -= 1.0

        # Normalize diagonal movement speed
        if move_x != 0.0 and move_y != 0.0:
            move_x *= 0.7071
            move_y *= 0.7071

        # Apply movement scaled by frame delta
        self.position.x += move_x * self.speed * delta
        self.position.y += move_y * self.speed * delta

        # Keep player within screen bounds
        self.position.x = max(30.0, min(1250.0, self.position.x))
        self.position.y = max(30.0, min(690.0, self.position.y))