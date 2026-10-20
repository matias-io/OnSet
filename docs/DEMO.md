# OnSet demo

A ten-minute walkthrough of OnSet with rekordbox: preparation in the app, then the live link and the full-screen visuals. Every step below was run on 2026-10-20 with the installed app, rekordbox 7.2.19 and the demo crate; what each step showed is noted in *italics*.

## Before the audience arrives

1. **rekordbox 7.2.19** open in Performance mode, 2-deck layout. Load *Miami 2 Ibiza* on deck 1 and *Lady Hear Me Tonight* on deck 2, both paused well before their end loops.
2. **OnSet** from its desktop shortcut (not from a terminal). The OnSet Live window opens with it; tick **Keep on top**.
3. **Demo crate analysed.** In Library, the **Analysed** view should list the crate. Light analysis takes about a minute a track; tick tracks across searches and press **Light analysis** once. The crate used here (17 tracks): Miami 2 Ibiza, Greyhound, Heaven Takes You Home, Lioness, Diamonds, Happiness Is So Sad (Swedish House Mafia); Losing It, Stop It, La Danza, STAY, World Hold On (FISHER rework); Lady Hear Me Tonight, Eternity, On My Mind, Light It Up, Drugs From Amsterdam, Call On Me.
4. **Visuals.** In OnSet Live press **Start visuals**. In the visuals launcher, **Show → Where the show goes**: the second screen (projector or laptop display), a style, then **Start show**. Press F for a window, Esc to come back.
5. Only one OnSet at a time, and no development builds running against the same workspace.

## The walkthrough

1. **Library** — 160 tracks, the Analysed / Review queue views, search and filters. Light analysis is local and quick; Deep analysis adds beat tracking, stems and model estimates in the WSL worker and takes much longer.
2. **Track review** (double-click *Losing It*) — the waveform with section bands (breakdown, groove, build), proposed cues A–L with times, **Snap to beats**, and keyboard review (A approves, R rejects, Ctrl+Z undoes). *Six proposed cues: intro 0.0 s, drop 7.7 s, breakdown 23.9 s, drop 2 38.4 s, outro 161.3 s, end loop 145.9–161.3 s.*
   - **Evidence** — character over time (drive, density, intensity, tension, contrast), where each source puts the parts *(energy baseline: first full groove bar 4, breakdown bar 12, later drop bar 20, outro bar 84)*, sourced descriptions with citations *(a Beatport source)*, and the spectrogram.
   - **Let Gemini listen…** sends the track's audio to Gemini for a structure read of its own, after a confirmation; **Read the spectrogram** sends only an image.
3. **Transitions** — Outgoing *Losing It*, Incoming *Stop It*, **Find options**. *EQ / bass swap 3.6, Filter transition 3.4, Long blend 3.2; tonal compatibility 1.00; e.g. start Stop It at 0:07 while Losing It is at 2:10, swap the bass at +13 s, out by +31 s.* **Render preview** makes a local rehearsal render *(42 s)*; **Audition both decks** plays both from their entry points. Leaving Incoming on "Best matches in library" ranks the whole library instead.
4. **Set planner** — with the Library on **Analysed**, the planner uses those tracks ("Plans from 17 visible library tracks"). Brief: 30 min, opening *Miami 2 Ibiza*, energy arc 4, 6, 8, 7, key lock on → **Build 3 options** *(about 90 s)*. *Option 1: 30:20, 11 tracks, timing validated, a planned transition under every step, closing Losing It → Greyhound.* Each option can be reordered, pinned, replanned from a point, auditioned, exported (JSON, CSV, HTML, PDF) or previewed as a rekordbox playlist.
   - **AI adviser** — tick a provider and **Compare with the planner**; it sends track profiles and the validated plans, never audio. *ChatGPT returned a valid alternative (9 shared tracks, same opener) and a critique: close on Heaven Takes You Home for the easing the brief asks for, and fewer backspins at intermediate level.*
5. **Live** — press play on deck 1 in rekordbox. OnSet Live names the track from rekordbox's own content id and shows BPM, key, time, the current and next section, the beat in the bar and the drop countdown; move MASTER to deck 2 and it follows. *Verified: Miami 2 Ibiza and Lady Hear Me Tonight, each named correctly as MASTER moved.*
6. **Visuals** — the show follows the same deck. *Liquid scene, overlay "Miami 2 Ibiza · playing · 125.0 BPM · 1:30 · phrase Verse (from Prep) · next Bridge in 37 · drop in 101 beats".*
7. **Cues to rekordbox** (optional, last) — approve a track's cues, **Preview memory cues** / **Preview hot cues** shows the exact change, then close rekordbox and **Apply these changes**; OnSet backs up the database first and the writer refuses while rekordbox runs.

## Rough edges seen while preparing it

- The visuals launcher shows its technical overlay (frame times, phrase line) and the taskbar over the show; switch the overlay off under **On screen** before an audience. The visuals' own interface is being moved into the app.
- Light analysis recomputes key: *Lady Hear Me Tonight* shows 10B (D major) in OnSet where rekordbox has 3B.
- The planner context sent to an AI adviser lacked four of the plan's transition IDs (ChatGPT pointed it out).
- *Ray Of Solar* fails light analysis ("Grid extends past the audio").
- The energy curve on plans needs rated or calibrated tracks.
- Multi-word typing through Computer Use goes via the clipboard and can paste something else when the clipboard changes underneath it.
