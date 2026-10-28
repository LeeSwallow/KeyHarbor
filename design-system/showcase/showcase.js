// Showcase entry: draw the sections, then wire the switchers.
import { SECTIONS } from "./sections/index.js";
import { renderSections } from "./render.js";
import { installControls } from "./controls.js";

renderSections(SECTIONS, document.getElementById("sections"));
installControls(document, location.search);
