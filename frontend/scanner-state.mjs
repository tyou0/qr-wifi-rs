export class ScanSession {
  #generation = 0;
  #phase = "idle";
  #suppressed = false;

  begin() {
    if (this.#phase !== "idle") return null;
    this.#phase = "starting";
    this.#suppressed = false;
    this.#generation += 1;
    return this.#generation;
  }

  activate(token) {
    if (!this.isCurrent(token) || this.#phase !== "starting") return false;
    this.#phase = "scanning";
    return true;
  }

  isScanning(token) {
    return this.isCurrent(token) && this.#phase === "scanning";
  }

  beginConnecting(token) {
    if (!this.isScanning(token)) return false;
    this.#phase = "connecting";
    return true;
  }

  // OS Wi-Fi changes cannot be cancelled once invoked. Keep the session locked
  // until settlement, but suppress stale UI after Stop or a tab switch.
  cancel() {
    if (this.#phase === "connecting") {
      this.#suppressed = true;
      return false;
    }
    this.#generation += 1;
    this.#phase = "idle";
    this.#suppressed = false;
    return true;
  }

  settleConnection(token) {
    if (!this.isCurrent(token) || this.#phase !== "connecting") return false;
    const shouldReport = !this.#suppressed;
    this.#phase = "idle";
    this.#suppressed = false;
    return shouldReport;
  }

  isCurrent(token) {
    return token === this.#generation;
  }

  get phase() {
    return this.#phase;
  }

  get token() {
    return this.#generation;
  }
}
