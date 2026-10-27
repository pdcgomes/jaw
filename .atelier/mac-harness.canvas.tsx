import { Badge, Button, Code, Gallery, Panel, Radio, Row, Stack, Tabs, Text, TextArea, Time, useCanvasState } from "atelier";
import { sweep, type Sweep } from "./mac-harness.data";

export const meta = { title: "Jaw for the Mac: the harness" };

type Scene = Sweep["scenes"][number];
/** A mark on a scene, with the hashes of the pictures it was made on. */
type Mark = { state: "right" | "needs-work"; note: string; hashes: string[]; at: string };
type Standing = "right" | "needs-work" | "changed" | "unseen";

const words: Record<Standing, string> = {
  right: "Right",
  "needs-work": "Needs work",
  changed: "Changed since your mark",
  unseen: "Not looked at",
};
const tones = { right: "success", "needs-work": "danger", changed: "warning", unseen: "muted" } as const;
const filters: { value: "all" | Standing; label: string }[] = [
  { value: "all", label: "Every scene" },
  { value: "needs-work", label: "Needs work" },
  { value: "changed", label: "Changed since marked" },
  { value: "unseen", label: "Not looked at" },
  { value: "right", label: "Right" },
];

const hashesOf = (scene: Scene) => scene.pictures.map((p) => p.hash);

/** Where a scene stands: unmarked, marked on the pictures it has now, or changed since its mark. */
function standing(scene: Scene, mark: Mark | null | undefined): Standing {
  if (!mark) return "unseen";
  const now = hashesOf(scene);
  const same = mark.hashes.length === now.length && mark.hashes.every((h, i) => h === now[i]);
  return same ? mark.state : "changed";
}

function ScenePanel({ scene, mark, onMark }: { scene: Scene; mark: Mark | null | undefined; onMark: (mark: Mark | null) => void }) {
  const now = standing(scene, mark);
  const keep = (state: Mark["state"], note: string) =>
    onMark({ state, note, hashes: hashesOf(scene), at: new Date().toISOString() });
  const picked = (state: Mark["state"]) => (mark?.state === state && now !== "changed" ? "solid" : "outline");
  return (
    <Panel title={scene.title} info={<Badge tone={tones[now]}>{words[now]}</Badge>}>
      <Stack gap={10}>
        <Text tone="secondary">{scene.synopsis}</Text>
        {scene.notes.map((n) => (
          <Text key={n.title} size="sm">
            {n.title}: {n.note}
          </Text>
        ))}
        <Gallery
          images={scene.pictures.map((p) => ({ path: p.path, alt: `${scene.title}, ${p.variant}`, caption: p.variant }))}
          columns={Math.min(Math.max(scene.pictures.length, 1), 4)}
        />
        <Row gap={8} align="center">
          <Button tone="success" variant={picked("right")} onClick={() => keep("right", "")}>
            Right
          </Button>
          <Button tone="danger" variant={picked("needs-work")} onClick={() => keep("needs-work", mark?.note ?? "")}>
            Needs work
          </Button>
          {mark ? (
            <Button variant="ghost" onClick={() => onMark(null)}>
              Clear
            </Button>
          ) : null}
        </Row>
        <TextArea
          label="What needs work"
          placeholder="What to change, and where"
          rows={2}
          value={mark?.note ?? ""}
          onChange={(note) => keep("needs-work", note)}
        />
      </Stack>
    </Panel>
  );
}

export default function HarnessReview() {
  const [marks, setMarks] = useCanvasState<Record<string, Mark | null>>("marks", {});
  const [filter, setFilter] = useCanvasState<string>("filter", "all");
  if (sweep.scenes.length === 0) {
    return (
      <Text tone="muted">
        Nothing has been swept yet: run <Code>mac/scripts/build-mac sweep</Code>.
      </Text>
    );
  }
  const count = (s: Standing) => sweep.scenes.filter((scene) => standing(scene, marks[scene.id]) === s).length;
  const options = filters.map(({ value, label }) => ({
    value,
    label: `${label} (${value === "all" ? sweep.scenes.length : count(value)})`,
  }));
  const pictures = sweep.scenes.reduce((n, scene) => n + scene.pictures.length, 0);
  const sections = [...new Set(sweep.scenes.map((s) => s.section))];
  const shown = (scene: Scene) => filter === "all" || standing(scene, marks[scene.id]) === filter;
  const mark = (id: string) => (m: Mark | null) => setMarks((previous) => ({ ...previous, [id]: m }));
  return (
    <Stack gap={16}>
      <Text tone="muted" size="sm">
        {sweep.scenes.length} scenes in {pictures} pictures, swept <Time at={sweep.made} />
      </Text>
      <Radio value={filter} onChange={setFilter} options={options} />
      <Tabs
        stateKey="section"
        tabs={sections.map((section) => {
          const scenes = sweep.scenes.filter((s) => s.section === section);
          const open = scenes.filter((s) => standing(s, marks[s.id]) !== "right").length;
          return {
            id: section,
            label: section,
            badge: open > 0 ? open : undefined,
            content: (
              <Stack gap={16}>
                {scenes.filter(shown).map((scene) => (
                  <ScenePanel key={scene.id} scene={scene} mark={marks[scene.id]} onMark={mark(scene.id)} />
                ))}
                {scenes.filter(shown).length === 0 ? <Text tone="muted">Nothing in this section under this filter.</Text> : null}
              </Stack>
            ),
          };
        })}
      />
    </Stack>
  );
}
