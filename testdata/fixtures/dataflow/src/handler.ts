export function handleRequest(input: string): object {
  const data = JSON.parse(input);
  const msg = `Received: ${data}`;
  console.log(msg);
  return data;
}

function processData(value: any): string {
  const result = value.name.toUpperCase();
  return result;
}
