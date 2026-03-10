export function handleRequest(input: string): object {
  const data = JSON.parse(input);
  console.log("Received:", data);
  return processData(data);
}

function processData(data: object): object {
  const result = transform(data);
  console.warn("Processed");
  return result;
}

function transform(data: object): object {
  return { ...data, transformed: true };
}
