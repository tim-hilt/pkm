import {
  type CloudBacking,
  ConnectFailed,
  connectMessage,
  statusText,
} from "../cloud/CloudBacking";

/**
 * The Cloud Backing page of the Settings: while Disconnected the WebDAV
 * form (Server URL, Username, Password, Connect) with the failure of the last
 * try; while Connected the provider with its server, the status with the time
 * of the last Sync, the Reconnect form while access is lost, and Disconnect.
 * WebDAV is the only Cloud Provider of the web app, so there is no picker.
 */
export class CloudBackingSection {
  readonly element = document.createElement("fieldset");
  private readonly content = document.createElement("div");
  private readonly status = document.createElement("p");
  /** Whether the Reconnect form is shown. */
  private reconnecting = false;

  constructor(private readonly cloud: CloudBacking) {
    this.element.className = "cloud-backing";
    const legend = document.createElement("legend");
    legend.textContent = "Cloud Backing";
    this.element.append(legend, this.content);
    cloud.onConnectionChange(() => this.render());
    // Only the status text changes with Sync; the form being typed in is left alone.
    cloud.onSyncChange(() => this.showStatus());
    this.render();
  }

  private showStatus(): void {
    const sync = this.cloud.syncReport;
    let text = statusText(sync, "WebDAV");
    if (sync.lastSync !== undefined)
      text += ` · Last Sync ${new Date(sync.lastSync).toLocaleTimeString()}`;
    this.status.hidden = !this.cloud.connection;
    this.status.className = "cloud-status" + (this.cloud.hasProblem ? " problem" : "");
    this.status.textContent = text;
    if (this.accessLost !== this.reconnecting) this.render();
  }

  /** Access lost needs the password again, which only the Reconnect form asks for. */
  private get accessLost(): boolean {
    return this.cloud.connection !== undefined && this.cloud.syncReport.status === "accessLost";
  }

  private render(error?: string): void {
    const connection = this.cloud.connection;
    this.reconnecting = this.accessLost;
    this.content.replaceChildren(
      connection ? this.connected(connection.server, error) : this.form(error),
    );
    this.showStatus();
  }

  private connected(server: string, error?: string): HTMLElement {
    const box = document.createElement("div");
    box.className = "cloud-fields";
    const row = document.createElement("p");
    row.className = "cloud-connected";
    const provider = document.createElement("strong");
    provider.textContent = "WebDAV";
    const detail = document.createElement("span");
    detail.className = "cloud-server";
    detail.textContent = server;
    row.append(provider, detail);
    const disconnect = button("Disconnect", "danger");
    disconnect.classList.add("cloud-disconnect");
    disconnect.addEventListener("click", () => void this.cloud.disconnect());
    box.append(row, this.status);
    if (this.reconnecting) box.append(this.reconnectForm(error));
    box.append(disconnect);
    return box;
  }

  /** Access lost: the password again, and Sync resumes where it stopped. */
  private reconnectForm(error?: string): HTMLElement {
    const form = document.createElement("form");
    form.className = "cloud-fields";
    const password = field("Password", "password", "", "current-password");
    const message = document.createElement("p");
    message.className = "cloud-error";
    message.setAttribute("role", "alert");
    message.textContent = error ?? "";
    message.hidden = !error;
    const reconnect = button("Reconnect", "primary");
    reconnect.type = "submit";
    const update = () => (reconnect.disabled = password.input.value === "");
    password.input.addEventListener("input", update);
    update();
    form.addEventListener("submit", async (event) => {
      event.preventDefault();
      reconnect.disabled = true;
      try {
        await this.cloud.reconnect(password.input.value);
      } catch (failure) {
        if (!(failure instanceof ConnectFailed)) throw failure;
        message.textContent = connectMessage(failure.reason);
        message.hidden = false;
        update();
      }
    });
    form.append(password.label, message, reconnect);
    return form;
  }

  private form(error?: string): HTMLElement {
    const form = document.createElement("form");
    form.className = "cloud-fields";
    const url = field("Server URL", "url", "https://example.com/dav", "off");
    const username = field("Username", "text", "", "username");
    const password = field("Password", "password", "", "current-password");
    const message = document.createElement("p");
    message.className = "cloud-error";
    message.setAttribute("role", "alert");
    message.textContent = error ?? "";
    message.hidden = !error;
    const connect = button("Connect", "primary");
    connect.type = "submit";
    const inputs = [url.input, username.input, password.input];
    const update = () => (connect.disabled = inputs.some((input) => input.value.trim() === ""));
    for (const input of inputs) input.addEventListener("input", update);
    update();
    form.addEventListener("submit", async (event) => {
      event.preventDefault();
      connect.disabled = true;
      connect.textContent = "Connecting…";
      try {
        await this.cloud.connect(url.input.value, username.input.value, password.input.value);
      } catch (failure) {
        if (!(failure instanceof ConnectFailed)) throw failure;
        message.textContent = connectMessage(failure.reason);
        message.hidden = false;
        connect.textContent = "Connect";
        update();
      }
    });
    form.append(url.label, username.label, password.label, message, connect);
    return form;
  }
}

function button(text: string, kind: string): HTMLButtonElement {
  const element = document.createElement("button");
  element.type = "button";
  element.className = kind;
  element.textContent = text;
  return element;
}

function field(title: string, type: string, hint: string, autocomplete: AutoFill) {
  const input = document.createElement("input");
  input.type = type;
  input.placeholder = hint;
  input.autocomplete = autocomplete;
  input.autocapitalize = "none";
  input.spellcheck = false;
  const label = document.createElement("label");
  label.className = "cloud-field";
  const text = document.createElement("span");
  text.textContent = title;
  label.append(text, input);
  return { label, input };
}
