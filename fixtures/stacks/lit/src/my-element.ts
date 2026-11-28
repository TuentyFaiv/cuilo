import { LitElement, html } from 'lit'
import { customElement } from 'lit/decorators.js'

/**
 * An example element.
 *
 * @slot - This element has a slot
 */
@customElement('my-element')
export class MyElement extends LitElement {
  render() {
    return html`<slot></slot>`
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'my-element': MyElement
  }
}
