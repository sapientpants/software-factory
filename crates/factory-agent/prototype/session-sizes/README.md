# Session sizes

Throwaway scripts behind `docs/research/15-session-sizes.md` (issue #19, D-22). They read the transcripts Claude Code keeps of its own sessions on one project and print aggregate figures: model calls, tool calls, active minutes, tokens and dollars at list price, per prompt, per ticket and per session. They change nothing and send nothing.

```
python3 -I measure.py ~/.claude/projects/<project> [<session to leave out>] [out.json] > measure.txt
python3 -I reprice.py out.json
python3 -I check.py ~/.claude/projects/<project>
```

- `measure.py`: the tables of the record. The optional session id is the session the measurement runs in, which is left out as in progress.
- `reprice.py`: each ticket priced again with its `claude-fable-5-2` tokens at the prices of `claude-opus-5-5`.
- `check.py`: Claude Code's own cost count against the dollars computed from the transcripts, and how long model calls and tool calls take.

No output is kept here. The JSON that `measure.py` writes holds session ids, and the transcripts are the operator's own.

As run on 2026-11-21 the scripts had the transcript directory and the session id written into them; here both are arguments. `reprice.py` now leaves out a ticket that has a prompt in progress, as `measure.py` does. Run again the same day with the arguments, the scripts gave the figures of the record.

The prices are those of `https://platform.claude.com/docs/en/about-claude/pricing.md` on 2026-11-21.
