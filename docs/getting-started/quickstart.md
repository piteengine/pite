---
icon: lucide/zap
---

# Quickstart

Get a 2D game running and make your first code change in under three minutes.

## Create a Project

Pite comes with starter templates to help you get moving quickly. Generate a new project using the `minimal-2d` template:

```sh title="Terminal"
pite new hello-pite --template minimal-2d
cd hello-pite
```

This scaffolds a complete project directory with a working 2D scene, spritesheet atlas, player and enemy scripts, audio, and UI elements.

!!! Tip
    If you omit the `--template` flag (`pite new my-game`), Pite creates a blank project with a minimal `pite.toml` manifest and empty folders.

## Launch the Visual Editor

To launch the interactive graphical editor, run `pite` inside your project folder:

```sh title="Terminal"
pite
```

*(You can also use `pite edit`).*

The Pite editor layout is designed around clarity and speed:

- **Center Panel**: The **Viewport** renders your game at its design resolution. Switch to the **Code** tab to view and edit Python scripts.
- **Top Bar**: Contains the menu bar (`Scene`, `Project`, `Help`) and playback controls: **Play (`▶`)** and **Stop (`■`)**.
- **Left Panel**: The **Scene Tree** hierarchy on top shows all nodes in the active scene (`Main`, `Player`, `Camera`, `HpLabel`, `HitBtn`, etc.), with the **Assets** browser below.
- **Right Panel**: The **Inspector** displays the editable properties (`props`) and script attachments for the currently selected node.
- **Bottom Panel**: The **Console** shows debug traces, log messages, and error diagnostics.

Press the **Play (`▶`)** button in the top-right corner (or press `Ctrl+R` / `F5`) to test the scene in the viewport.

!!! Note
    Keep the mouse pointer inside the viewport game area to send keyboard and mouse inputs to your running game. Moving the mouse outside the game area allows the editor shortcuts and code pane to retain priority.

- Move the player using the **Arrow keys** or **WASD**.
- Press **Space** or click the **HIT** button on screen to hit the enemy. Notice how the enemy flinches, the sound effect plays, and the HP counter decrements!

## Run Directly from the Terminal

You don't need the editor open to test your game. Run the standalone engine runtime straight from your terminal:

```sh title="Terminal"
pite run
```

This launches your project's main scene directly in a lightweight window with full hot reload active.

??? Tip "Testing specific scenes"
    You can test any scene in isolation by passing its path to `pite run`:

    ```sh title="Terminal"
    pite run scenes/enemy.ptscn
    ```

## Edit a Script and Watch Hot Reload

Pite scripts are standard Python files powered by an embedded Python 3.12 runtime. You can edit scripts inside the editor's **Code** tab or using any external code editor such as VS Code, Zed, or PyCharm.

Open `scripts/player.py` and locate the `speed` variable on line 6:

```python title="scripts/player.py" hl_lines="6"
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
```

Change `speed: float = 200.0` to `speed: float = 500.0`, then save the file (`Ctrl+S`).

If your game is running in `pite run` or inside the editor, **hot reload** immediately reloads the Python module in place. Tap the arrow keys: your player character now moves across the screen at high speed without restarting the game.

## Check Project Health

Pite includes a built-in static validator that inspects your manifests, scenes, asset references, and Python scripts:

```sh title="Terminal"
pite check
```

`pite check` verifies:

- Scene node hierarchies, missing parents, and invalid node types.
- Missing or moved `res://` assets (and suggests automatic re-import commands).
- Script syntax and unresolved dependencies.
- Manifest configuration syntax and schema.

!!! Tip "Strict mode for CI"
    Run `pite check --strict` to treat warnings as errors, ideal for automated test pipelines and GitHub Actions.

## Export a Standalone Build

When you are ready to distribute your game, Pite packages a standalone, single-folder release with a bundled Python runtime and runner binary:

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

You can export to any supported platform from any development OS — for example, building Windows and macOS releases while working on Linux, or producing Linux builds from Windows. Pite handles fetching the target platform's runner binary and embedded Python environment automatically without requiring external SDKs, cross-compilers, or virtual machines.

The output lands in `dist/` containing an executable launcher and all required assets, ready to distribute to players.

## Next Steps

Now that you have seen how Pite projects work, dive deeper into the engine:

<div class="grid cards" markdown>

- :lucide-folder-tree: **[Project Structure](project-structure)**
    Explore the files, folders, and conventions in a Pite project.

- :material-rocket-launch: **[Your First Game](../tutorials/first-game)**
    Build a complete 2D Gem Collector game from scratch step-by-step.

- :material-hammer-wrench: **[Editor Guide](../guides/editor)**
    Master the visual scene editor, tree manipulation, and shortcuts.

- :material-share-variant: **[Export Guide](../guides/export)**
    Learn how cross-platform exports and standalone player bundling work.

</div>
