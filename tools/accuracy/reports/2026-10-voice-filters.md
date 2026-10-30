# Voice filters: VoxLock vs Alkove, and the echo gate (#32, #14)

Measured 2026-10-30 on p620 by the lead. **Decision: no filter.** Neither
VoxLock nor Alkove meets the decision rule, so no follow-up issue is opened.
`echo_gate` stays off (#14): it does well on synthetic bleed and nothing on
the real room recording.

## Set

The owner chose (2026-10-30) to be measured without their own recordings, so
the set is built from LibriSpeech test-clean (CC BY 4.0, md5
`32fa31d27d2e1cad72775fee3f4849a9`) with `tools/accuracy/libri_mix.py`:
10 calls of 62–99 s, every 4th speaker as the owner, one other speaker as
the remote side. Mic = owner + the remote speech 120 ms late at -12 dB
(speaker bleed); system = the remote speech; two remote turns per call are
spoken over by the owner (double talk). Owner sample = the 12 s enrollment
from #31's set, never a trial utterance. 1213 owner words, 973 remote words.

Synthetic bleed is a pure delayed copy: easier than a real room (no reverb,
no speaker distortion). The real-room check below shows how much that
matters.

Versions: sherpa-onnx 2.0.1 (this branch), VoxLock INT8 from
`speech-enhancement-models/sherpa-onnx-voxlock-int8.tar.bz2`
(`sha256-vLlH+ehnbT6WP2BIsAiFBchPCEO/3IjLJt3jJk2BVws=`), Alkove 0.3.2 and
numpy 2.2.6 wheels pinned by hash (README), Parakeet TDT 0.6b v3 INT8,
echo gate from `src/audio/echo_gate.rs` (`examples/echo_gate_file.rs`).

## Per call

Each cell: owner words lost / remote words on the mic / bleed lines, from
`score.py --parakeet`. RTF = CPU seconds per second of audio (p620, under
load from a parallel run; VoxLock's is far above 1 either way).

| Call | No filter | VoxLock | Alkove | Echo gate | RTF VoxLock / Alkove |
|---|---|---|---|---|---|
| 1089 | 25 / 9 / 1 | 4 / 9 / 0 | 25 / 0 / 0 | 4 / 0 / 0 | 2.77 / 0.11 |
| 1284 | 30 / 12 / 0 | 7 / 0 / 0 | 5 / 5 / 0 | 5 / 0 / 0 | 2.56 / 0.11 |
| 2094 | 39 / 27 / 2 | 10 / 37 / 2 | 13 / 27 / 2 | 7 / 0 / 0 | 2.54 / 0.10 |
| 2830 | 1 / 9 / 0 | 19 / 0 / 0 | 47 / 0 / 0 | 17 / 0 / 0 | 2.56 / 0.11 |
| 3729 | 7 / 1 / 0 | 7 / 0 / 0 | 18 / 11 / 1 | 7 / 1 / 0 | 2.53 / 0.16 |
| 4970 | 49 / 0 / 0 | 3 / 0 / 0 | 3 / 0 / 0 | 2 / 0 / 0 | 2.50 / 0.11 |
| 5639 | 65 / 7 / 2 | 81 / 0 / 0 | 42 / 1 / 1 | 40 / 0 / 0 | 2.52 / 0.10 |
| 6829 | 10 / 1 / 1 | 0 / 2 / 1 | 0 / 0 / 0 | 0 / 0 / 0 | 2.55 / 0.10 |
| 7176 | 15 / 0 / 0 | 1 / 0 / 0 | 14 / 0 / 0 | 2 / 0 / 0 | 2.55 / 0.10 |
| 8455 | 24 / 7 / 0 | 8 / 0 / 0 | 4 / 0 / 0 | 5 / 11 / 0 | 2.52 / 0.11 |
| **All** | **265 / 73 / 6** | **140 / 48 / 3** | **171 / 44 / 4** | **89 / 12 / 0** | **2.56 / 0.11** |

No variant invented a line on silence. "Owner words lost" without a filter
is high because a double-talk line classified as bleed counts all its owner
words as lost, and rare names (Stephanos, Dedalos) are misheard in every
variant.

## Decision rule (spec section 4)

A filter needs all three: more foreign words removed than #31's voice
check would label `room`; at most 1 in 50 owner words lost; under half a
core.

| | Foreign words removed vs no filter | #31 would remove | Owner words lost that no filter kept | CPU | Pass |
|---|---|---|---|---|---|
| VoxLock | 37 | ~39 | 37 (3.1%) | 2.56 cores | no: all three |
| Alkove | 46 | ~39 | 59 (4.9%) | 0.11 core | no: owner words |

"#31 would remove": 44 remote words sit in whole bleed lines a `room`
label can take away; #31's step 7 caught 88% of speaker voices, so ~39.

What each did wrong: VoxLock cannot run live on p620 (2.5× real time), made
call 2094 worse (27 → 37 remote words) and cut 81 owner words in 5639.
Alkove removed the most bleed but cut owner words in double talk (2830:
1 → 47).

## Echo gate (#14's pending decision)

On the synthetic calls the gate looks excellent: remote words on the mic
73 → 12, bleed lines 6 → 0, owner words lost 265 → 89 (it unmasks owner
lines that bleed had swamped; it also lost 28 words the ungated run kept,
2.3%). On the real room recording from 28 Oct (`remote-only-tap.wav`,
speakers playing six remote lines, no owner speech) it does nothing useful:
remote words on the mic 15 → 19, bleed lines 4 → 4. The synthetic bleed is
a clean delayed copy the gate's envelope correlation finds easily; a real
room is not. **`echo_gate` stays off by default.** PipeWire echo
cancellation (`echoCancel.enable`) stays the measure for real rooms.
