import pite


class GameTimer(pite.Timer):
    seconds_left: int = 30

    def _ready(self):		
        # Timer has a built-in timeout signal that loops every wait_time
        self.get_node(".").timeout.connect(self.on_second_tick)

    def on_second_tick(self):
        if self.seconds_left > 0:
            self.seconds_left -= 1
            label = self.get_node("../TimeLabel")
            label.text = f"Time: {self.seconds_left}"

            if self.seconds_left == 0:
                self.game_over()

    def game_over(self):
        label = self.get_node("../TimeLabel")
        label.text = "Time's Up!"
        label.color = "#EF4444"

        # Stop player movement
        player = self.get_node("../Player")
        player.speed = 0.0