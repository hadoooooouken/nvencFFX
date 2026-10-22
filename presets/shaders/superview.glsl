//!HOOK SCALED
//!BIND HOOKED
//!DESC SuperView nonlinear stretch

vec4 hook()
{
    vec2 pos = HOOKED_pos;

    // Convert horizontal texture coordinate from [0, 1] to [-1, 1]
    float x = pos.x * 2.0 - 1.0;

    // Calculate the distance-from-edge factor
    float distanceFromEdge = abs(x) * 0.5 + 0.5;

    // Apply nonlinear horizontal stretching
    float t = distanceFromEdge * distanceFromEdge;
    x = mix(x / 0.75, x, t);

    // Convert back to [0, 1] and sample the image
    pos.x = x * 0.5 + 0.5;

    return HOOKED_tex(pos);
}
