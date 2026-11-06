export const gateway = process.env.E2E_GATEWAY === "1";
export const front = gateway ? 3000 : 13000;
export const backend = gateway ? 8090 : 18090;
