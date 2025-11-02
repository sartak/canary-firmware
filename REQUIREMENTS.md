# Canary Keyboard Firmware Requirements

## Hardware Configuration

- Split keyboard design
- Small key count
- Supports both traditional typing and stenography-style chording
- Either half can be the primary side connected to the host
- Split communication via TRRS using UART serial
- Emits media keys, mouse events, etc

## Core Features

### Configuration

- Optimized for my particular 34-key Ferris Sweep layout and hardware (RP2040)
- Supports compile-time configuration (using TOML format) for layout, chords, etc

### Modifiers

- Hold any letter/symbol/number key for 200ms to emit cmd-modified version
- Any key can be configured to emit modifiers, either one-shot or held (e.g. `space` down, `a` down, `a` up -> outputs `A`)

### Chording

Chording triggers arbitrary behaviors when multiple keys are pressed simultaneously.

#### Text Output
- Combine multiple keys simultaneously to output entire words
- Example: `c+n+d` -> `consider `
- Can output arbitrary text, not just single words
- Trailing spaces controlled by `exact` property:
  - `exact: false` (default): Adds trailing space (used for words/phrases)
  - `exact: true`: No trailing space (used for punctuation, symbols, code identifiers)
- Text can include backspace characters (`\b`) to delete previous characters

#### Behavior Chords
Chords can trigger actions instead of text output:
- Enable oneshot modifiers
- Enable oneshot layers
- Mouse clicks (left/right button)
- Custom keyboard actions (e.g. toggle "hard mode")
- Enter bootloader mode
- Reboot keyboard

##### Delete Word / Undo Chord
- Ordinarily `space+backspace` combo sends `alt+backspace` to delete preceding word
- When `space+backspace` is pressed immediately after a chord, delete that chord's exact output
- Handles multi-word chords correctly
- Handles chords without trailing spaces (e.g., in code identifiers)

#### Shift Behavior
- Holding shift OR including shift in combo capitalizes first letter by default
- Per-chord shift customization:
  - **Default**: First letter capitalized
  - **Disabled**: Never capitalize (e.g., `https://` never becomes `Https://`)
  - **Custom**: Define alternate behavior (e.g., `champs-elysees` -> `Champs-Élysées`)

#### Smart Punctuation
After typing a chord:
- `.` deletes trailing space, adds period + space, enables sticky shift for next character/chord
- `,` deletes trailing space, adds comma + space, no shift behavior

#### Hold-for-Alternate

Each chord can define custom behavior when held for 200ms (separate from tap):

**Extends Key Layout**:
- Example: Two left thumb keys
  - Tap: Left mouse click
  - Hold: cmd-modified left click
- Example: Two inner thumb keys (`delete-word` behavior)
  - Tap: Delete word
  - Hold: Oneshot Hyper

#### Four Variants Per Chord
Each chord can have up to 4 base behaviors:
1. Base (tap)
2. Shifted (tap with shift)
3. Held (hold 200ms)
4. Held + Shifted

Each variant can have its own dup cycling expansions or behavior.

#### Layer Restrictions
Chords can be restricted to specific layers via the `layers` property. When set, chord only activates on specified layers.

### Duplicate Key (⨧)

#### Basic Duplication
- Standalone tap repeats previous keystroke with modifiers
- Enables more efficient double-letter typing (e.g., `suc` + ⨧ + `es` + ⨧' -> `success`)

#### Chord Cycling
Tapping ⨧ immediately after a text-output chord cycles through predefined expansions:
- Example: `t+n+s` -> `thanks `
  - ⨧ -> `thank you `
  - ⨧ -> `Thank you very much!`
  - ⨧ -> `thanks ` (cycles back)

#### Behavior Chord Repetition
- Ordinarily, tapping ⨧ after a behavior chord repeats it
- Customizable

### Hard Mode

To improve muscle memory, the keyboard has a hard mode that enforces the best typing method.

- Hard mode toggled with either compile-time configuration or a chord behavior
- If word is typed letter-by-letter but it matches a chord, take some action
   - If the chord is not configured as "hard mode", then resume as normal
   - Otherwise delete the output of the chord
   - Immediately after deletion, Tab key behavior changes:
      - First Tab: Emit the chord's key combo into text field
      - Second Tab: Erase the displayed combo
      - `space+backspace` also erases the displayed combo
- Forbid duplicate repeat keys in letter-by-letter (must use ⨧ key)

### Sidechannel

Keyboard emits separate data stream (JSONL format) containing:
- Individual key up and down events
- Chord activations
- Layer changes
- Setting changes
- Which half is primary (for split keyboard)

### Split Keyboard
Communication protocol:
- Primary (USB-connected) always listens on UART RX
- Secondary transmits key state changes when they occur
- No polling needed - secondary-driven updates
- Primary never sends to secondary (unidirectional communication)

Primary/secondary detection:
- Automatic via USB connection detection
- If USB enumerated → Primary role (initialize USB HID + UART RX)
- If no USB → Secondary role (initialize UART TX only)
- Either half can be primary depending on which is plugged into host
