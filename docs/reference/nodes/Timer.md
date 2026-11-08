# Timer

Fires the engine-owned `timeout` signal every `wait_time` seconds.

**Inherits:** `Node`

## Properties

### Timer

| Property | Type | Default | Description |
|---|---|---|---|
| `wait_time` | number | `1` | Seconds between `timeout` emissions. Non-positive disables the timer. |
| `current_elapsed` | number | `0` | Seconds accumulated toward the next fire. Reset to 0 on fire; scripts can reset it manually. |

