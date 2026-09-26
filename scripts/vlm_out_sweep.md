# Sweep panel VLM verification


## sweep_panel

Based on the provided screenshot of the "Global Sweep Optimizer" panel:

1) **Panel Header:** The specific text "Global Sweep Optimizer — provably optimal multi-venue execution" is **not visible** in the screenshot. The panel begins immediately with the stat tiles.
2) **Stat Tiles:** All five requested tiles are present and populated:
    *   **Open Plans:** 2 (live optimal sweeps)
    *   **Sweep P&L (paper):** 74.01 (30 fills · 0 expired)
    *   **Best Net Edge:** 3.04 (bps after fees)
    *   **Avg Net Edge:** 2.47 (bps over filled plans)
    *   **Avg Venues:** 2.7 (per filled plan)
3) **Plans Table:**
    *   **Headers:** MKT, OPTIMAL VENUE SPLIT (BUY → SELL), NET BPS, EV, PROFIT, NOTIONAL are all correctly displayed.
    *   **Venue Chips:** Yes, chips with buy/sell labels are clearly visible (e.g., "buy BY$10.0k", "sell HL$10.0k", "buy BN$7.0k", "buy BS$3.0k").
4) **Execution Log:**
    *   **Headers:** STATUS, EXECUTED SPLIT, DETECTED, P&L, LEGS, TIME are all present.
    *   **Fill Rows:** Yes, there are multiple rows showing a status of "fill" with corresponding execution data for ETH and SOL.
5) **Rendering Defects:**
    *   There is significant **overlapping/clipping** on the right side of the screen where "Arb fill" notification cards overlap with the right edge of the main table/log area.
    *   Some text within those overlapping notification cards appears slightly garbled or cut off by the edge of the viewport.

**Verdict:**
PANEL OK: yes
DEFECTS: Overlapping notification cards on the right margin clipping the main content area; missing panel title/header as described in the prompt.

## sweep_full

**Verification Results:**

1) **Panel Header:** The panel header reads **"HyperLight Terminal"**, not "Global Sweep Optimizer — provably optimal multi-venue execution". (The text "Global Sweep Optimizer" does not appear anywhere in the image).

2) **Stat Tiles:** Five tiles are visible with the following values:
*   **OPEN PLANS:** 2 ("Live optimal sweeps")
*   **SWEEP P&L (PAPER):** 74.01 ("30 fills · 0 expired")
*   **BEST NET EDGE:** 3.04 ("bps after fees")
*   **AVG NET EDGE:** 2.47 ("bps over filled plans")
*   **AVG VENUES:** 2.7 ("per filled plan")

3) **Plans Table:**
*   **Headers:** Mkt / Optimal venue split (buy → sell) / Net bps / EV / Profit / Notional.
*   **Venue Chips:** Yes, visible. Examples include "buy BY$10.0k", "sell HL$10.0k", "buy BN$3.0k", and "sell BY$10.0k".

4) **Execution Log:**
*   **Columns:** Status / Executed split / Detected / P&L / Legs / Time.
*   **Fill Rows:** Yes, multiple rows are present, all showing a status of "fill".

5) **Rendering Defects:**
*   There is significant **text overlap/clipping** on the right side of the screen where dark tooltip-like boxes (e.g., "Arb fill #601...", "Sol: buy BY...") are rendered directly over each other and over the main content area.
*   Some text in the "Optimal venue split" column of the plans table is slightly cramped but generally legible.

***

**PANEL OK:** no (Header mismatch)
**DEFECTS:** Header text is incorrect ("HyperLight Terminal" instead of "Global Sweep Optimizer"); severe overlapping/tooltips clutter on the right side of the panel.
