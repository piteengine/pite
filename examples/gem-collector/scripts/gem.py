import random
import pite


class Gem(pite.Area2D):
    score: int = 0

    def _ready(self):
        # Connect the built-in collision signal
        self.body_entered.connect(self.on_body_entered)

    def on_body_entered(self, body_id: str):
        # Check if the colliding body is the player
        if body_id == "player":
            self.collect()

    def collect(self):
        # Play the pickup sound effect
        pite.play("res://sfx/coin.wav")

        # Increment score and update the HUD
        self.score += 1
        label = self.get_node("../ScoreLabel")
        label.text = f"Score: {self.score}"

        # Relocate the gem to a random position on screen
        new_x = random.uniform(80.0, 1200.0)
        new_y = random.uniform(80.0, 640.0)
        self.position = (new_x, new_y)