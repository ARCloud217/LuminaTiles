// 主题令牌快照：把 getComputedStyle 解析出的实际颜色写到 .tmp/theme-snapshot.json，
// 供改样式前后用 diff 对比，确认推导结果没有被改动。
const fs = await import("node:fs");
const targets = ["--color-bg", "--color-bg-deep", "--color-surface", "--color-surface-raised", "--color-ink", "--color-ink-muted", "--color-ink-faint", "--color-line", "--color-line-strong", "--color-accent", "--color-accent-soft", "--color-accent-glow", "--color-wash", "--color-wash-soft"];
const list = await (await fetch("http://127.0.0.1:" + process.argv[2] + "/json/list")).json();
const page = list.find((item) => item.type === "page" && /127\.0\.0\.1:\d+\/$/.test(item.url));
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve, reject) => { ws.onopen = resolve; ws.onerror = reject; });
const result = await new Promise((resolve) => {
  ws.onmessage = (event) => resolve(JSON.parse(event.data));
  ws.send(JSON.stringify({ id: 1, method: "Runtime.evaluate", params: {
    expression: `(() => {
      const style = getComputedStyle(document.documentElement);
      const probe = document.createElement("canvas").getContext("2d");
      const out = {};
      for (const name of ${JSON.stringify(targets)}) {
        const raw = style.getPropertyValue(name).trim();
        probe.clearRect(0, 0, 1, 1);
        probe.fillStyle = "#000";
        probe.fillStyle = raw;
        probe.fillRect(0, 0, 1, 1);
        const data = probe.getImageData(0, 0, 1, 1).data;
        out[name] = { raw, rgb: Array.from(data).join(",") };
      }
      return out;
    })()`, returnByValue: true } }));
});
ws.close();
const value = result.result?.result?.value;
if (!value) throw new Error(JSON.stringify(result));
fs.writeFileSync(".tmp/theme-snapshot.json", JSON.stringify(value, null, 2), "utf8");
console.log(Object.entries(value).map(([k, v]) => k + " = " + v.rgb).join("\n"));
