import { mount } from "svelte";
import "./ui";
import App from "./App.svelte";

export default mount(App, { target: document.getElementById("root")! });
