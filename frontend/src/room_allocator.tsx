/* @refresh reload */
import { render } from "solid-js/web";

import "../../assets/scss/custom.scss";
import "../../assets/scss/custom.scss";
import "./index.css";
import DrawRoomAllocator from "./DrawRoomAllocator";

const root = document.getElementById("root");

render(() => <DrawRoomAllocator />, root!);
