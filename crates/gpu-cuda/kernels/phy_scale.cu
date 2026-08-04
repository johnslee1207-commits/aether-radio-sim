extern "C" __global__ void phy_scale(float *data, int n, float scale) {
    int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i < n) {
        data[i] *= scale;
    }
}
