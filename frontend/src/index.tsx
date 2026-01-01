/* @refresh reload */
import { render } from "solid-js/web";
import "../../assets/scss/custom.scss";
import "./index.css";
import DrawEditor from "./DrawEditor";

const root = document.getElementById("root");

render(() => <DrawEditor />, root!);
