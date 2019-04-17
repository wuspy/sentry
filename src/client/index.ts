import * as nipplejs from 'nipplejs';

//const socket = new WebSocket(`ws://${window.location.hostname}:8080`);
const socket = new WebSocket(`ws://127.0.0.1:8080`);
const rtc = new RTCPeerConnection();

const joystickSize = 200;
const joystick = nipplejs.create({
    zone: document.getElementById("joystick-zone"),
    color: "white",
    size: joystickSize,
});

let joystickVector = {pitch: 0, yaw: 0};

const sendJoystickVector = () => {
    socket.send(JSON.stringify(joystickVector));
}

const sendCommand = (command: string) => {
    socket.send(JSON.stringify({command: command}));
}

socket.onopen = (event: Event) => {
    console.log("Connection established");
};

socket.onmessage = (event: MessageEvent) => {
    console.log(JSON.parse(event.data));
};

joystick.on("move", (event: nipplejs.EventData, data: nipplejs.JoystickOutputData) => {
    joystickVector = {
        pitch: data.distance / (joystickSize / 2) * Math.sin(data.angle.radian),
        yaw: data.distance / (joystickSize / 2) * Math.cos(data.angle.radian),
    };
});

joystick.on("end", () => {
    joystickVector = {pitch: 0, yaw: 0};
});

document.getElementById("fire-button").onclick = (event: MouseEvent) => {
    sendCommand("fire");
};

setInterval(sendJoystickVector, 20);
