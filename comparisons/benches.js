let n = 0;
let sum = 0;
const start = performance.now();

while (n < 100000000) {
    sum += n;
    n += 1;
}

const end = performance.now();
console.log(`Finished in ${(end - start).toFixed(4)}ms`);