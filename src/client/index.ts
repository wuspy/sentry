import * as nipplejs from 'nipplejs';

const socket = new WebSocket(`ws://${window.location.hostname}:8081`);
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

const joystickInterval = setInterval(sendJoystickVector, 20);

const displayMessage = (message: string, name: string = "") => {
    const element = document.getElementById("message");
    element.setAttribute("data-message-name", name);
    element.innerHTML = message;
    element.style.display = "block";
}

const hideMessage = (name: string = "") => {
    const element = document.getElementById("message");
    if (name === "" || element.getAttribute("data-message-name") === name) {
        element.style.display = "none";
    }
}

let hasErrored = false;
const fatalError = (message: string) => {
    if (!hasErrored) {
        hasErrored = true;
        displayMessage(message, "error");
        clearInterval(joystickInterval);
        joystick.destroy();
        document.getElementById("button-container").remove();
        socket.close();
        socket.onmessage = undefined;
    }
}

const sendCommand = (command: string) => {
    socket.send(JSON.stringify({command: command}));
}

socket.onopen = (event: Event) => {
    hideMessage("waiting_for_connection");
}

socket.onerror = (event: Event) => {
    fatalError("Error communicating with server");
}

socket.onclose = (event: Event) => {
    fatalError("Connection lost");
}

socket.onmessage = (event: MessageEvent) => {
    if (event.data["rtc_answer"]) {
        rtc.setRemoteDescription(new RTCSessionDescription(event.data["rtc_answer"]));
    }
    if (event.data["queue_position"]) {
        if (event.data["queue_position"] == 0) {
            hideMessage("already_in_use");
        } else {
            displayMessage("Someone else is already in control of the sentry", "already_in_use");
        }
    }
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

rtc.createOffer()
    .then((offer) => {
        rtc.setLocalDescription(new RTCSessionDescription(offer));
        socket.send(JSON.stringify({rtc_offer: offer}));
    });

rtc.onicecandidate = (event) => {
    if (event.candidate) {
        socket.send(JSON.stringify({ice_candidate: event.candidate}));
    }
}

rtc.onicecandidateerror = (event) => {
    fatalError(`Could not connect to video server (${event.errorText})`);
}

rtc.ontrack = (event) => {
    (document.getElementById("player") as HTMLVideoElement).srcObject = event.streams[0];
}

displayMessage("Waiting for connection...", "waiting_for_connection");