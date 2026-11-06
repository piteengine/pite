---
icon: lucide/rocket
---

# Your First Game

Build a complete 2D Gem Collector game from scratch using nodes, collision detection, audio, and Python scripting.

In this tutorial, you will build a complete mini-game: a player character navigating the screen to collect randomly spawning gems. When the player touches a gem:

1. A sound effect plays.
2. The score counter increments on the HUD.
3. The gem relocates to a fresh random location on the screen.

You can follow every step either visually inside the **Pite Editor** or by **directly editing project files**. Use the content tabs in each section to follow your preferred approach!

---

## Create the Project

Start by scaffolding a blank project from your terminal:

```sh title="Terminal"
pite new gem-collector
cd gem-collector
```

This creates a clean workspace with starter directories for `assets/`, `scenes/`, and `scripts/`.

### Prepare the Assets

Get your sprite images and sound effects into your project:

=== "In Editor"
    1. Launch the editor:
        ```sh title="Terminal"
        pite
        ```
    2. In the **Assets panel** (bottom-left), click the **:lucide-folder-plus: button** in the header (or right-click root folder and choose **New Folder…**), name it `sfx`, and confirm.
    3. Drag your `player.png` and `gem.png` image files from your operating system's file manager directly into the `assets` folder in the Assets panel.
    4. Drag your `coin.wav` audio file from your OS file manager directly into the `sfx` folder.

    The editor automatically ingests the files and registers them in the asset manifest.

=== "Direct File Edit"
    !!! Note "Text-First Workflow"
        Because Pite files are designed to be human-readable and Git-friendly (plain TOML for manifests and scenes, standard Python scripts, and standard media formats), you can comfortably build, inspect, and modify your entire game using command-line tools and your preferred text editor without needing the visual editor.

    Create the audio directory from your terminal:
    ```sh title="Terminal"
    mkdir sfx
    ```

    Copy your image and audio files into `assets/` and `sfx/`:

    - `assets/player.png`: Sprite for the player character (e.g. 48×48 px).
    - `assets/gem.png`: Sprite for the collectible gem (e.g. 32×32 px).
    - `sfx/coin.wav`: Sound effect played when picking up a gem.

!!! Tip
    You can use any PNG images and WAV/OGG audio files you have on hand, or grab free public-domain assets from [Kenney](https://kenney.nl).

---

## Project Manifest

Inspect the project settings and design resolution:

=== "In Editor"
    1. In the editor menu bar, open **Project > Project Settings…**.
    2. Confirm that **Window Width** is `1280` and **Window Height** is `720`.
    3. Optionally enter your name in the **Author** field.
    4. Click **Save** to write the configuration to `pite.toml`.

=== "Direct File Edit"
    Open `pite.toml` in your code editor to inspect the generated project metadata:

    ```toml title="pite.toml" hl_lines="6"
    [project]
    name = "gem-collector"
    pite_version = "0.3.0"
    main_scene = "res://scenes/main.ptscn"
    python = "3.12"
    author = "Your Name"
    icon = ""
    window_width = 1280
    window_height = 720
    ```

    The scaffolded defaults already configure `window_width = 1280` and `window_height = 720` as the target design resolution, which the engine scales uniformly to fit any monitor.

---

## Build the Main Scene

Construct the scene hierarchy containing the camera, HUD label, player body, and gem trigger:

=== "In Editor"
    Double-click `scenes/main.ptscn` in the Assets panel to open it. The Scene Tree displays the root node `Main`.

    1. **Add Camera**:
        - Select `Main` in the Scene Tree, click **+** (or right-click `Main` > **Add Child Node…**), search for `Camera2D`, name it `Camera`, and click **Create**.
        - In the Inspector under `Node2D` section, set `position` to `X: 640.0`, `Y: 360.0`.
    2. **Add Score Label**:
        - Select `Main`, click **+**, choose `Label`, name it `ScoreLabel`, and click **Create**.
        - In the Inspector under `Label`, set `text` to `Score: 0`, `font_size` to `32.0`, and `color` to `#FACC15`. Under `Node2D`, set `position` to `X: 120.0`, `Y: 50.0`.
    3. **Add Player**:
        - Select `Main`, click **+**, choose `CharacterBody2D`, name it `Player`. Under `Node2D`, set `position` to `X: 640.0`, `Y: 360.0`.
        - Select `Player`, click **+**, add a child `Sprite2D` named `Sprite`. Drag `assets/player.png` from the Assets panel into the `texture` field in the Inspector.
        - Select `Player`, click **+**, add a child `CollisionShape2D` named `Collider`. In the Inspector under `CollisionShape2D`, set `shape` to `capsule` and `radius` to `25.0`, `height` to `35.0`, `position` to `X: 0`, `Y: 20`.

            !!! Info
                This properties are based on our sample sprite for the player, you can continue configuring it to make it more similar to your player sprite.
    4. **Add Gem**:
        - Select `Main`, click **+**, choose `Area2D`, name it `Gem`. Under `Node2D`, set `position` to `X: 400.0`, `Y: 250.0`.
        - Select `Gem`, click **+**, add a child `Sprite2D` named `Sprite`. Drag `assets/gem.png` from the Assets panel into its `texture` field.
        - Select `Gem`, click **+**, add a child `CollisionShape2D` named `Collider`. Under `CollisionShape2D`, set `shape` to `"circle"` and `radius` to `16.0`.

            !!! Info
                This properties are based on our sample sprite for the gem, you can continue configuring it to make it more similar to your gem sprite.

    Press <kbd>Ctrl</kbd> + <kbd>S</kbd> to save the scene!

=== "Direct File Edit"
    When you create a blank project, `scenes/main.ptscn` starts with a single root `Node2D` named `Main` (lines 1–8). Open `scenes/main.ptscn` and expand the scene tree by adding the nodes for the camera, HUD label, player, and gem (highlighted below):

    ```toml title="scenes/main.ptscn" hl_lines="9-92"
    format_version = 1
    root = "root"

    [[node]]
    id = "root"
    type = "Node2D"
    name = "Main"

    [[node]]
    id = "cam"
    type = "Camera2D"
    name = "Camera"
    parent = "root"

    [node.props]
    position = [640.0, 360.0]

    [[node]]
    id = "score_label"
    type = "Label"
    name = "ScoreLabel"
    parent = "root"

    [node.props]
    color = "#FACC15"
    font_size = 32.0
    position = [120.0, 50.0]
    text = "Score: 0"

    [[node]]
    id = "player"
    type = "CharacterBody2D"
    name = "Player"
    parent = "root"

    [node.props]
    position = [640.0, 360.0]

    [node.script]
    path = "res://scripts/player.py"
    class = "Player"

    [[node]]
    id = "player_sprite"
    type = "Sprite2D"
    name = "Sprite"
    parent = "player"

    [node.props]
    texture = "res://assets/player.png"

    [[node]]
    id = "player_collider"
    type = "CollisionShape2D"
    name = "Collider"
    parent = "player"

    [node.props]
    shape = "circle"
    radius = 24.0

    [[node]]
    id = "gem"
    type = "Area2D"
    name = "Gem"
    parent = "root"

    [node.props]
    position = [400.0, 250.0]

    [node.script]
    path = "res://scripts/gem.py"
    class = "Gem"

    [[node]]
    id = "gem_sprite"
    type = "Sprite2D"
    name = "Sprite"
    parent = "gem"

    [node.props]
    texture = "res://assets/gem.png"

    [[node]]
    id = "gem_collider"
    type = "CollisionShape2D"
    name = "Collider"
    parent = "gem"

    [node.props]
    shape = "circle"
    radius = 16.0
    ```

### Scene Breakdown

- **Camera (`cam`)**: Centers the game view at `(640, 360)`, framing the entire 1280×720 world.
- **ScoreLabel (`score_label`)**: A `Label` node positioned near the top-left to show the current score.
- **Player (`player`)**: A `CharacterBody2D` containing a `Sprite2D` graphic and a circular `CollisionShape2D` collider, attached to `scripts/player.py`.
- **Gem (`gem`)**: An `Area2D` trigger zone containing its own sprite and collider, attached to `scripts/gem.py`.

---

## Player Movement Script

Create the player script with smooth 8-directional movement:

```python title="scripts/player.py"
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
```

### Attaching the Script to the player

=== "In Editor"
    1. In the Assets panel, select the `scripts` folder and click **+ File** (or right-click `scripts` > **New File…**).
    2. Select the **Script (.py)** template, name the file `player.py`, and click **Create**.
    3. Double-click `scripts/player.py` in the Assets panel to open it in the **Code** tab, and paste the code above. Save with <kbd>Ctrl</kbd> + <kbd>S</kbd>.
    4. Switch back to the **Viewport** tab, select the `Player` node in the Scene Tree, and drag `scripts/player.py` from the Assets panel onto the `Player` node to attach it.

=== "Direct File Edit"
    1. Create `scripts/player.py` in your code editor and paste the Python code above.
    2. In `scenes/main.ptscn`, ensure the `Player` node includes the script reference:
        ```toml
        [node.script]
        path = "res://scripts/player.py"
        class = "Player"
        ```

---

## Gem Collection Script

Create the gem script to handle collection events, sound playback, score tracking, and random relocation:

```python title="scripts/gem.py"
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
```

### Attaching the Script to the gem

=== "In Editor"
    1. In the Assets panel under `scripts`, click **+ File**, choose **Script (.py)**, name it `gem.py`, and click **Create**.
    2. Double-click `scripts/gem.py` to open it in the **Code** tab, and paste the script above. Save with <kbd>Ctrl</kbd> + <kbd>S</kbd>.
    3. In the Scene Tree, drag `scripts/gem.py` from the Assets panel onto the `Gem` node to attach it.

=== "Direct File Edit"
    1. Create `scripts/gem.py` in your code editor and paste the Python code above.
    2. In `scenes/main.ptscn`, ensure the `Gem` node includes the script reference:
        ```toml
        [node.script]
        path = "res://scripts/gem.py"
        class = "Gem"
        ```

---

## Test the Game

Now test your game in action:

=== "In Editor"
    Click the **Play (`▶`)** button in the top-right corner of the editor (or press ++ctrl+r++ / ++f5++).

    Keep the mouse pointer inside the game viewport to send input:

    - Move with **WASD** or **Arrow keys**.
    - Touch the gem to collect it and hear the pickup sound effect.
    - Watch your score counter increment and the gem teleport to a new spot!

=== "Direct Terminal Run"
    Reimport assets and run the standalone runtime:
    ```sh title="Terminal"
    pite check --reimport-assets
    pite run
    ```

---

## Add a Countdown Timer

To turn the collector into a complete game, add a countdown timer that gives players 30 seconds to collect as many gems as possible.

### Update the Scene

=== "In Editor"
    1. **Add TimeLabel**:
        - Select `Main` in the Scene Tree, click **+**, add a `Label` named `TimeLabel`.
        - In the Inspector under `Label`, set `text` to `Time: 30`, `font_size` to `28.0`, and `color` to `#FFFFFF`. Under `Node2D`, set `position` to `X: 1150.0`, `Y: 50.0`.
    2. **Add GameTimer**:
        - Select `Main`, click **+**, add a `Timer` named `GameTimer`.
        - In the Inspector under `Timer`, set `wait_time` to `1.0`.
    3. Press ++ctrl+s++ to save!

=== "Direct File Edit"
    Append the `TimeLabel` and `GameTimer` nodes to the end of `scenes/main.ptscn` (highlighted below):

    ```toml title="scenes/main.ptscn" hl_lines="3-26"
    # (append to the end of scenes/main.ptscn)

    [[node]]
    id = "time_label"
    type = "Label"
    name = "TimeLabel"
    parent = "root"

    [node.props]
    color = "#FFFFFF"
    font_size = 28.0
    position = [1150.0, 50.0]
    text = "Time: 30"

    [[node]]
    id = "game_timer"
    type = "Timer"
    name = "GameTimer"
    parent = "root"

    [node.props]
    wait_time = 1.0

    [node.script]
    path = "res://scripts/game_timer.py"
    class = "GameTimer"
    ```

### Create the Timer Script

```python title="scripts/game_timer.py"
import pite


class GameTimer(pite.Timer):
    seconds_left: int = 30

    def _ready(self):
        # Timer has a built-in timeout signal that loops every wait_time
        self.timeout.connect(self.on_second_tick)

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
```

=== "In Editor"
    1. In the Assets panel under `scripts`, click **+ File**, create `game_timer.py`, and paste the script above. Save with <kbd>Ctrl</kbd> + <kbd>S</kbd>.
    2. Drag `scripts/game_timer.py` from the Assets panel onto `GameTimer` in the Scene Tree to attach it.
    3. Press **Play (`▶`)** to test your complete timed arcade game!

=== "Direct File Edit"
    Save `scripts/game_timer.py` in your code editor and run:
    ```sh title="Terminal"
    pite run
    ```

---

## Export Your Game

Package your finished game for distribution:

=== "Windows"
    ```sh title="Terminal"
    pite export windows
    ```

=== "Linux"
    ```sh title="Terminal"
    pite export linux
    ```

=== "macOS"
    ```sh title="Terminal"
    pite export macos
    ```

The exported build is placed in `dist/gem-collector-<platform>/` containing the standalone game binary, runtime, and all packed assets ready to share with friends.

---

## What's Next?

Congratulations on finishing your first Pite game! Here are a few ways to expand it:

- Add level obstacles using `StaticBody2D` nodes or design tiled levels with [Tilemaps](../reference/scene-format#tilemaps).
- Add animated coin frames using a sprite sheet atlas (see the [Assets Guide](../guides/assets)).
- Use the visual [Editor](../guides/editor) to arrange levels interactively.
