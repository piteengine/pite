from __future__ import annotations

class Node:
    node_path: str
    def get_node(self, path: str) -> NodeProxy: ...
    def _ready(self) -> None: ...
    def _process(self, delta: float) -> None: ...

class NodeProxy:
    position: tuple[float, float]

class Node2D(Node):
    position: tuple[float, float]

class Sprite2D(Node2D):
    texture: str

class Camera2D(Node2D): ...

class Timer(Node):
    wait_time: float
