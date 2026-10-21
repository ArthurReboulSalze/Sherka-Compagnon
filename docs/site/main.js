// Sherka Companion — project site interactions.

const COMPANIONS = [
  { id: "sherka", name: "Sherka", line: "Calm, warm, attentive. An onboard computer from a retrofuturistic voyage." },
  { id: "botsy", name: "Botsy", line: "Your curious little tinkerer." },
  { id: "alba", name: "Alba", line: "White & gold android. Playful, lively, adventurous." },
  { id: "kit", name: "Kit", line: "Precision and deadpan wit." },
  { id: "minerva", name: "Minerva", line: "Armored android. Fiery, impatient, sharp-tongued." },
];
const MOODS = ["neutral", "joy", "sadness", "anger", "surprise", "reflection", "tenderness"];
const img = (id, mood) => `site/img/companions/${id}/${mood}.webp`;
const label = (mood) => mood[0].toUpperCase() + mood.slice(1);

function setupCast() {
  const list = document.querySelector("[data-cast-list]");
  const moods = document.querySelector("[data-moods]");
  if (!list || !moods) return;
  const layers = [...document.querySelectorAll("[data-portrait] img")];
  const name = document.querySelector("[data-cast-name]");
  const line = document.querySelector("[data-cast-line]");
  const index = document.querySelector("[data-cast-index]");
  let current = { companion: 0, mood: "neutral" };
  let front = 0;

  COMPANIONS.forEach((c, i) => {
    const button = document.createElement("button");
    button.className = "cast-item";
    button.type = "button";
    button.setAttribute("role", "tab");
    button.innerHTML = `<img src="${img(c.id, "neutral")}" alt="" loading="lazy">
      <div><span class="num">0${i + 1}</span><strong>${c.name}</strong><span>${c.line}</span></div>`;
    button.addEventListener("click", () => show(i, current.mood));
    list.append(button);
  });

  MOODS.forEach((mood) => {
    const button = document.createElement("button");
    button.className = "mood";
    button.type = "button";
    button.setAttribute("role", "radio");
    button.dataset.mood = mood;
    button.textContent = label(mood);
    button.addEventListener("click", () => show(current.companion, mood));
    moods.append(button);
  });

  // Crossfade between two stacked layers, like the desktop avatar.
  function show(ci, mood) {
    const c = COMPANIONS[ci];
    const src = img(c.id, mood);
    const next = layers[1 - front];
    const reveal = () => {
      layers[front].classList.remove("on");
      next.classList.add("on");
      front = 1 - front;
    };
    next.alt = `${c.name}, ${mood} expression`;
    if (next.getAttribute("src") === src && next.complete) reveal();
    else {
      next.onload = reveal;
      next.src = src;
    }
    current = { companion: ci, mood };
    name.textContent = c.name;
    line.textContent = c.line;
    index.textContent = `0${ci + 1} / ${c.name}`;
    list.querySelectorAll(".cast-item").forEach((el, i) => el.setAttribute("aria-selected", String(i === ci)));
    moods.querySelectorAll(".mood").forEach((el) => el.setAttribute("aria-checked", String(el.dataset.mood === mood)));
  }

  layers[0].src = img("sherka", "neutral");
  layers[0].alt = "Sherka, neutral expression";
  show(0, "neutral");
  // Warm the cache so expression changes fade instead of popping.
  addEventListener("load", () => COMPANIONS.forEach((c) => MOODS.forEach((m) => { new Image().src = img(c.id, m); })));
}

// Golden connectors from each capability card to the central orb.
function setupEcosystem() {
  const eco = document.querySelector("[data-eco]");
  if (!eco) return;
  const svg = eco.querySelector(".eco-links");
  const core = eco.querySelector(".eco-core");
  const ns = "http://www.w3.org/2000/svg";

  function draw() {
    svg.replaceChildren();
    if (getComputedStyle(svg).display === "none") return;
    const box = eco.getBoundingClientRect();
    const c = core.getBoundingClientRect();
    const cx = c.left + c.width / 2 - box.left;
    const cy = c.top + c.height / 2 - box.top;
    const radius = c.width * 0.43;
    eco.querySelectorAll("[data-link]").forEach((card) => {
      const r = card.getBoundingClientRect();
      const fromLeft = card.dataset.link === "right";
      const x1 = (fromLeft ? r.right : r.left) - box.left;
      const y1 = r.top + r.height / 2 - box.top;
      const angle = Math.atan2(y1 - cy, x1 - cx);
      const x2 = cx + Math.cos(angle) * radius;
      const y2 = cy + Math.sin(angle) * radius;
      const mid = (x1 + x2) / 2;
      const d = `M${x1},${y1} C${mid},${y1} ${mid},${y2} ${x2},${y2}`;
      const trail = document.createElementNS(ns, "path");
      trail.setAttribute("d", d);
      trail.setAttribute("class", "trail");
      const line = document.createElementNS(ns, "path");
      line.setAttribute("d", d);
      const dir = Math.sign(x2 - x1);
      const head = document.createElementNS(ns, "path");
      head.setAttribute("d", `M${x2 - dir * 12},${y2 - 9} L${x2},${y2} L${x2 - dir * 12},${y2 + 9}`);
      svg.append(trail, line, head);
    });
  }

  new ResizeObserver(draw).observe(eco);
  document.fonts?.ready.then(draw);
}

setupCast();
setupEcosystem();
