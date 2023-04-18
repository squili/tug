import { For, createResource, createSignal } from 'solid-js';
import styles from './App.module.css';

const BASE = import.meta.env.VITE_BASE;

async function fetchTodos() {
  const response = await fetch(BASE);
  return await response.json();
}

function App() {
  const [todos, { refetch }] = createResource(fetchTodos, { initialValue: {} });
  const [disabled, setDisabled] = createSignal(false);

  let input = (<input></input>);

  const req = (url, method, body) => {
    setDisabled(true);
    fetch(url, {
      method,
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify(body),
    }).then(() => {
      refetch().then(() => {
        setDisabled(false);
      })
    })
  }

  return (
    <main class={styles.App}>
      <h1>Todo MVC</h1>
      <For each={Object.entries(todos())}>{([id, value]) => {
        let input = null;
        input = (<input value={value.content} onBlur={() => {
          if (input.value !== value.content)
            req(`${BASE}?id=${id}`, 'PUT', { 'content': input.value, 'complete': value.complete });
        }}></input>);
        return <div>
          <button disabled={disabled()} onClick={() => {
            req(`${BASE}?id=${id}`, 'PUT', { 'content': value.content, 'complete': !value.complete });
          }}>{value.complete ? '☑' : '☐'}</button>
          {input}
          <button disabled={disabled()} onClick={() => {
            req(`${BASE}?id=${id}`, 'DELETE', {});
          }}>X</button>
        </div>
      }
      }</For>
      <div>
        {input}
        <button disabled={disabled()} onClick={() => {
          const value = input.value;
          input.value = '';
          req(BASE, 'POST', { 'content': value, 'complete': false });
        }}>+</button>
      </div>
    </main>
  );
}

export default App;
