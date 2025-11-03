# Canary Firmware - Implementation Plan

## Milestone 1: Core USB
**Goal**: Basic firmware structure with USB keyboard functionality and debugging

- [x] Sidechannel for debugging
- [x] Initialize USB HID keyboard descriptor
- [x] Initialize USB HID mouse descriptor
- [x] Basic key matrix scanning
- [x] Raw key input → USB HID output
- [ ] Debouncing

## Milestone 2: Split Keyboard
**Goal**: Split keyboard support

- [ ] Handedness configuration
- [ ] USB connection detection for primary/secondary selection
- [ ] Half-duplex UART serial communication on single pin (GP2/D2 via TRRS)
- [ ] Key state synchronization
- [ ] Verify both halves work independently as primary
- [ ] Sidechannel: emit which half is primary

## Milestone 3: Configuration
**Goal**: Compile-time configuration

- [ ] Define TOML configuration schema for keyboard layout
- [ ] Parse TOML at compile time
- [ ] Map physical key positions to logical keys
- [ ] Support for basic keycodes (letters, numbers, symbols)

## Milestone 4: Layers
**Goal**: Implement layer functionality

- [ ] Add layers to configuration
- [ ] Layer switching (hold, oneshot, toggle)
- [ ] Sidechannel: emit layer changes

## Milestone 5: Modifiers
**Goal**: Implement modifier key behavior

- [ ] Oneshot modifiers (tap key, next key is modified)
- [ ] Hold-for-modifier (space→shift, backspace→ctrl when held with another key)
- [ ] Combo modifiers (space+backspace→hyper)
- [ ] Hold-any-key-for-cmd (200ms threshold to emit cmd-modified version)
- [ ] Modifier state tracking and proper HID reporting
- [ ] Sidechannel: emit modifiers

## Milestone 6: Duplicate Key
**Goal**: Implement ⨧ key for repeating previous input

- [ ] Track last emitted keystroke (including modifiers)
- [ ] Standalone ⨧ tap repeats last keystroke
- [ ] ⨧ tap can take additional modifiers

## Milestone 7: Basic Chording
**Goal**: Detect simultaneous key presses and trigger simple behaviors

- [ ] TOML schema for chord definitions
- [ ] Compile-time error on combo conflicts
- [ ] Combo detection
- [ ] Behavior: Text output
- [ ] `exact` property (trailing space vs no trailing space)
- [ ] Sidechannel: emit chord activations with input keys and triggered behavior

## Milestone 8: Advanced Chord Features
**Goal**: Shift behavior, hold-for-alternate, and behavior chords

- [ ] Layer restrictions for chords (`layers` property)
- [ ] Shift behavior for chords
- [ ] Hold behavior for chords
- [ ] Behavior: Oneshot modifier activation
- [ ] Behavior: Oneshot layer activation
- [ ] Behavior: Mouse button clicks
- [ ] Behavior: Toggle hard mode
- [ ] Behavior: Bootloader mode
- [ ] Behavior: Reboot keyboard
- [ ] Behavior: Delete word / undo chord

## Milestone 9: Smart Punctuation & Chord Cycling
**Goal**: Context-aware punctuation and dup-based chord expansion

- [ ] Track most recent chord activation
- [ ] Smart punctuation after chords (`.` and `,` handling)
- [ ] Sticky shift after period
- [ ] Text chord cycling with ⨧
- [ ] Behavior chord repetition with ⨧

## Milestone 10: Hard Mode
**Goal**: Training mode to enforce optimal typing habits

- [ ] Buffer recent character-by-character output
- [ ] Word matching against chord dictionary
- [ ] Per-chord hard mode configuration flag
- [ ] Word deletion when match detected
- [ ] Tab key behavior override
- [ ] Forbid manual double letters (must use ⨧)
