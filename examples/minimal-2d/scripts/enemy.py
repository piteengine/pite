import pite


class Enemy(pite.Node2D):
    def _ready(self):
        self.hp = 3
        self.get_node("../Player").hit.connect(self.on_player_hit)

    def on_player_hit(self, damage):
        self.hp = self.hp - damage
        x, y = self.position
        self.position = (x - 5.0 * damage, y)
