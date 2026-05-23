import time

n = 837799
steps = 0
start = time.perf_counter()

while n > 1:
    if n % 2 == 0:
        n //= 2
    else:
        n = n * 3 + 1
    steps += 1

end = time.perf_counter()
print(steps)
print(f"Finished in {(end - start) * 1000:.4f}ms")