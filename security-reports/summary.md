# Security Scan Summary

**Image**: crabcache:security-test  
**Scan Date**: Thu Jan  1 23:21:39 -03 2026  
**Scan Results Directory**: ./security-reports

## Scan Results

- ✅ Docker Scout: Results available in docker-scout.json
- ❌ Trivy: Scan failed or not available
- ❌ Grype: Scan failed or not available

## Next Steps

1. Review the JSON reports for detailed vulnerability information
2. Update Dockerfile to address any critical vulnerabilities
3. Rebuild the image after applying fixes
4. Re-run this scan to verify fixes

## Security Best Practices

- Regularly update base images
- Keep dependencies up to date
- Use minimal base images when possible
- Run containers as non-root users
- Implement proper network segmentation
- Monitor for security updates

